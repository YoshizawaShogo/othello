use rand::prelude::IndexedRandom;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::time::Duration;

use crate::model::{BOARD_SIZE, Cell, Pos};

const OPENAI_RESPONSES_URL: &str = "https://api.openai.com/v1/responses";
const REQUEST_TIMEOUT_SECS: u64 = 90;
const RETRY_ATTEMPTS: usize = 3;

// Approximate USD per 1M tokens for gpt-5-mini; adjust as pricing changes.
const INPUT_USD_PER_1M: f64 = 0.25;
const OUTPUT_USD_PER_1M: f64 = 2.00;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    pub fn name(self) -> &'static str {
        match self {
            Self::Easy => "Easy",
            Self::Normal => "Normal",
            Self::Hard => "Hard",
        }
    }

    fn reasoning_effort(self) -> &'static str {
        match self {
            Self::Easy => "low",
            Self::Normal => "medium",
            Self::Hard => "high",
        }
    }

    fn temperature(self) -> f64 {
        match self {
            Self::Easy => 1.0,
            Self::Normal => 0.6,
            Self::Hard => 0.2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub estimated_cost_usd: f64,
}

#[derive(Clone, Debug)]
pub struct CpuMoveResult {
    pub pos: Pos,
    pub usage: Option<TokenUsage>,
    pub fallback_used: bool,
    pub note: Option<String>,
}

pub struct OpenAiClient {
    model: String,
    api_key: String,
    http: Client,
}

#[derive(Clone, Copy)]
struct RequestOptions {
    include_temperature: bool,
    include_reasoning: bool,
}

impl OpenAiClient {
    pub fn new(api_key: String, model: String) -> Result<Self, String> {
        let http = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
        Ok(Self {
            model,
            api_key,
            http,
        })
    }

    pub fn choose_move(
        &self,
        board: &[[Cell; BOARD_SIZE]; BOARD_SIZE],
        turn: Cell,
        legal_moves: &[Pos],
        difficulty: Difficulty,
    ) -> CpuMoveResult {
        if legal_moves.is_empty() {
            return CpuMoveResult {
                pos: Pos::new(0, 0),
                usage: None,
                fallback_used: true,
                note: Some("CPU had no legal moves".to_string()),
            };
        }

        match self.call_openai(board, turn, legal_moves, difficulty) {
            Ok((pos, usage)) => CpuMoveResult {
                pos,
                usage,
                fallback_used: false,
                note: None,
            },
            Err(err) => {
                let fallback = *legal_moves
                    .choose(&mut rand::rng())
                    .unwrap_or(&legal_moves[0]);
                CpuMoveResult {
                    pos: fallback,
                    usage: None,
                    fallback_used: true,
                    note: Some(format!("CPU fallback move used: {err}")),
                }
            }
        }
    }

    fn call_openai(
        &self,
        board: &[[Cell; BOARD_SIZE]; BOARD_SIZE],
        turn: Cell,
        legal_moves: &[Pos],
        difficulty: Difficulty,
    ) -> Result<(Pos, Option<TokenUsage>), String> {
        let primary = RequestOptions {
            include_temperature: true,
            include_reasoning: true,
        };
        match self.call_openai_once(board, turn, legal_moves, difficulty, primary) {
            Ok(ok) => Ok(ok),
            Err(err) => {
                if is_unsupported_temperature_error(&err) {
                    let second = self.call_openai_once(
                        board,
                        turn,
                        legal_moves,
                        difficulty,
                        RequestOptions {
                            include_temperature: false,
                            include_reasoning: true,
                        },
                    );
                    match second {
                        Ok(ok) => Ok(ok),
                        Err(err2) if is_incomplete_max_output_tokens_error(&err2) => self
                            .call_openai_once(
                                board,
                                turn,
                                legal_moves,
                                difficulty,
                                RequestOptions {
                                    include_temperature: false,
                                    include_reasoning: false,
                                },
                            ),
                        Err(err2) => Err(err2),
                    }
                } else if is_incomplete_max_output_tokens_error(&err) {
                    let second = self.call_openai_once(
                        board,
                        turn,
                        legal_moves,
                        difficulty,
                        RequestOptions {
                            include_temperature: true,
                            include_reasoning: false,
                        },
                    );
                    match second {
                        Ok(ok) => Ok(ok),
                        Err(err2) if is_unsupported_temperature_error(&err2) => self
                            .call_openai_once(
                                board,
                                turn,
                                legal_moves,
                                difficulty,
                                RequestOptions {
                                    include_temperature: false,
                                    include_reasoning: false,
                                },
                            ),
                        Err(err2) => Err(err2),
                    }
                } else {
                    Err(err)
                }
            }
        }
    }

    fn call_openai_once(
        &self,
        board: &[[Cell; BOARD_SIZE]; BOARD_SIZE],
        turn: Cell,
        legal_moves: &[Pos],
        difficulty: Difficulty,
        options: RequestOptions,
    ) -> Result<(Pos, Option<TokenUsage>), String> {
        let board_text = board_to_ascii(board);
        let legal = legal_moves
            .iter()
            .map(|p| p.notation())
            .collect::<Vec<_>>()
            .join(", ");
        let turn_name = turn.name();
        let user_prompt = format!(
            "You are choosing one Othello move. Return only one coordinate in a1-h8 format.\\nTurn: {turn_name}\\nLegal moves: {legal}\\nBoard:\\n{board_text}"
        );

        let mut payload = json!({
            "model": self.model,
            "reasoning": { "effort": difficulty.reasoning_effort() },
            "input": [
                {
                    "role": "system",
                    "content": [
                        {
                            "type": "input_text",
                            "text": "You are an Othello CPU. Reply with exactly one legal move in lowercase like d3. No extra text."
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "input_text",
                            "text": user_prompt
                        }
                    ]
                }
            ]
        });
        if options.include_temperature {
            payload["temperature"] = json!(difficulty.temperature());
        }
        if !options.include_reasoning
            && let Some(obj) = payload.as_object_mut()
        {
            obj.remove("reasoning");
        }

        let (status, body) = self.request_json_with_retry(&payload)?;

        if !status.is_success() {
            let msg = body
                .get("error")
                .and_then(|v| v.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("unknown API error");
            return Err(format!("OpenAI API error ({status}): {msg}"));
        }

        let text = extract_output_text(&body).ok_or_else(|| {
            format!(
                "No output text in response. {}",
                response_diagnostics(&body)
            )
        })?;
        let parsed =
            parse_move(&text).ok_or_else(|| format!("Could not parse move from: {text:?}"))?;

        if !legal_moves.contains(&parsed) {
            return Err(format!(
                "Model returned illegal move: {}",
                parsed.notation()
            ));
        }

        let usage = parse_usage(&body);
        Ok((parsed, usage))
    }
}

fn is_unsupported_temperature_error(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("unsupported parameter") && lower.contains("temperature")
}

fn is_incomplete_max_output_tokens_error(err: &str) -> bool {
    err.contains("incomplete_reason=max_output_tokens")
}

fn format_reqwest_error(err: &reqwest::Error) -> String {
    let mut tags = Vec::new();
    if err.is_timeout() {
        tags.push("timeout");
    }
    if err.is_connect() {
        tags.push("connect");
    }
    if err.is_request() {
        tags.push("request");
    }
    if err.is_decode() {
        tags.push("decode");
    }
    if err.is_status() {
        tags.push("status");
    }
    let tag = if tags.is_empty() {
        "unknown".to_string()
    } else {
        tags.join("|")
    };
    match err.url() {
        Some(url) => format!("{tag}: {err} (url={url})"),
        None => format!("{tag}: {err}"),
    }
}

fn preview_body(raw: &str, max: usize) -> String {
    let mut s = raw.replace('\n', "\\n");
    if s.len() > max {
        s.truncate(max);
        s.push_str("...");
    }
    s
}

impl OpenAiClient {
    fn request_json_with_retry(
        &self,
        payload: &Value,
    ) -> Result<(reqwest::StatusCode, Value), String> {
        let attempts = RETRY_ATTEMPTS;
        let mut last_err = String::new();

        for attempt in 1..=attempts {
            let resp = self
                .http
                .post(OPENAI_RESPONSES_URL)
                .bearer_auth(&self.api_key)
                .header("Content-Type", "application/json")
                .json(payload)
                .send();

            let resp = match resp {
                Ok(r) => r,
                Err(err) => {
                    last_err = format!(
                        "Request failed on attempt {attempt}/{attempts}: {} (timeout={}s)",
                        format_reqwest_error(&err),
                        REQUEST_TIMEOUT_SECS
                    );
                    if attempt < attempts {
                        let backoff_ms = 400 * attempt as u64;
                        std::thread::sleep(Duration::from_millis(backoff_ms));
                        continue;
                    }
                    return Err(last_err);
                }
            };

            let status = resp.status();
            let raw = resp.text().map_err(|e| {
                format!(
                    "Failed to read response body (status {status}) on attempt {attempt}/{attempts}: {}",
                    format_reqwest_error(&e)
                )
            })?;

            let body: Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(e) => {
                    let preview = preview_body(&raw, 600);
                    return Err(format!(
                        "Failed to parse JSON response (status {status}) on attempt {attempt}/{attempts}: {e}; body_preview={preview}"
                    ));
                }
            };

            return Ok((status, body));
        }

        Err(last_err)
    }
}

pub fn parse_move(raw: &str) -> Option<Pos> {
    for token in raw.split(|c: char| {
        c.is_whitespace() || c == ',' || c == ';' || c == ':' || c == '(' || c == ')'
    }) {
        let candidate = token.trim().to_lowercase();
        let bytes = candidate.as_bytes();
        if bytes.len() < 2 {
            continue;
        }
        let file = bytes[0];
        let rank = bytes[1];
        if (b'a'..=b'h').contains(&file) && (b'1'..=b'8').contains(&rank) {
            return Some(Pos::new((file - b'a') as usize, (rank - b'1') as usize));
        }
    }
    None
}

fn extract_output_text(body: &Value) -> Option<String> {
    if let Some(text) = body.get("output_text").and_then(Value::as_str)
        && !text.trim().is_empty()
    {
        return Some(text.to_string());
    }
    if let Some(parts) = body.get("output_text").and_then(Value::as_array) {
        let joined = parts
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("");
        if !joined.trim().is_empty() {
            return Some(joined);
        }
    }

    let output = body.get("output")?.as_array()?;
    for item in output {
        if let Some(text) = item.get("text").and_then(Value::as_str)
            && !text.trim().is_empty()
        {
            return Some(text.to_string());
        }
        let Some(content) = item.get("content").and_then(Value::as_array) else {
            continue;
        };
        for c in content {
            if let Some(text) = c.get("text").and_then(Value::as_str)
                && !text.trim().is_empty()
            {
                return Some(text.to_string());
            }
            if let Some(text) = c
                .get("text")
                .and_then(|v| v.get("value"))
                .and_then(Value::as_str)
                && !text.trim().is_empty()
            {
                return Some(text.to_string());
            }
        }
    }
    None
}

fn response_diagnostics(body: &Value) -> String {
    let status = body
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let incomplete_reason = body
        .get("incomplete_details")
        .and_then(|v| v.get("reason"))
        .and_then(Value::as_str)
        .unwrap_or("-");
    let output_types = body
        .get("output")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("type").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_else(|| "-".to_string());
    let body_preview = {
        let mut s = body.to_string();
        if s.len() > 600 {
            s.truncate(600);
            s.push_str("...");
        }
        s
    };
    format!(
        "status={status}, incomplete_reason={incomplete_reason}, output_types={output_types}, body_preview={body_preview}"
    )
}

fn parse_usage(body: &Value) -> Option<TokenUsage> {
    let usage = body.get("usage")?;
    let input_tokens = usage.get("input_tokens")?.as_u64()?;
    let output_tokens = usage.get("output_tokens")?.as_u64()?;
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(input_tokens + output_tokens);
    Some(TokenUsage {
        input_tokens,
        output_tokens,
        total_tokens,
        estimated_cost_usd: estimate_cost_usd(input_tokens, output_tokens),
    })
}

pub fn estimate_cost_usd(input_tokens: u64, output_tokens: u64) -> f64 {
    (input_tokens as f64 / 1_000_000.0) * INPUT_USD_PER_1M
        + (output_tokens as f64 / 1_000_000.0) * OUTPUT_USD_PER_1M
}

fn board_to_ascii(board: &[[Cell; BOARD_SIZE]; BOARD_SIZE]) -> String {
    let mut out = String::new();
    for (y, row) in board.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            let ch = match cell {
                Cell::Black => 'B',
                Cell::White => 'W',
                Cell::Empty => '.',
            };
            out.push(ch);
            if x + 1 < BOARD_SIZE {
                out.push(' ');
            }
        }
        if y + 1 < BOARD_SIZE {
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_move_works() {
        assert_eq!(parse_move("d3"), Some(Pos::new(3, 2)));
        assert_eq!(parse_move("d3\n"), Some(Pos::new(3, 2)));
        assert_eq!(parse_move("move: h8"), Some(Pos::new(7, 7)));
        assert_eq!(parse_move("z9"), None);
        assert_eq!(parse_move(""), None);
    }

    #[test]
    fn difficulty_mapping_expected() {
        assert_eq!(Difficulty::Easy.reasoning_effort(), "low");
        assert_eq!(Difficulty::Normal.reasoning_effort(), "medium");
        assert_eq!(Difficulty::Hard.reasoning_effort(), "high");
        assert!(Difficulty::Easy.temperature() > Difficulty::Hard.temperature());
    }

    #[test]
    fn cost_estimation_non_zero() {
        let cost = estimate_cost_usd(10_000, 2_000);
        assert!(cost > 0.0);
    }

    #[test]
    fn extract_output_text_from_value_shape() {
        let body = json!({
            "output": [
                {
                    "type": "message",
                    "content": [
                        {"type": "output_text", "text": {"value": "d3"}}
                    ]
                }
            ]
        });
        assert_eq!(extract_output_text(&body).as_deref(), Some("d3"));
    }

    #[test]
    fn extract_output_text_skips_reasoning_items() {
        let body = json!({
            "output": [
                {"type": "reasoning", "summary": []},
                {
                    "type": "message",
                    "content": [
                        {"type": "output_text", "text": "c3"}
                    ]
                }
            ]
        });
        assert_eq!(extract_output_text(&body).as_deref(), Some("c3"));
    }
}
