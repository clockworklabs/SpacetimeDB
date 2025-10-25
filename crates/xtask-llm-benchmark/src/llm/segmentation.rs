use crate::llm::types::Vendor;
use serde_json::json;

#[derive(Clone, Debug)]
pub struct Segment<'a> {
    pub role: &'a str,    // "system" | "user" | "assistant"
    pub text: String,     // segment contents
    pub weight: f32,      // relative importance for allocation (>= 0)
    pub min_chars: usize, // hard floor per segment after trimming
    pub keep: bool,       // never drop entirely
}

impl<'a> Segment<'a> {
    pub fn new(role: &'a str, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
            weight: 1.0,
            min_chars: 0,
            keep: false,
        }
    }
    pub fn keep(mut self) -> Self {
        self.keep = true;
        self
    }
    pub fn weight(mut self, w: f32) -> Self {
        self.weight = w.max(0.0);
        self
    }
    pub fn min_chars(mut self, n: usize) -> Self {
        self.min_chars = n;
        self
    }
}

/// Conservative heuristic
pub fn estimate_tokens(s: &str) -> usize {
    let by_bytes = s.as_bytes().len() / 4;
    let by_words = s.split_whitespace().count() / 2;
    by_bytes.max(by_words).max(1)
}

// ---------------- Builders ----------------

/// OpenAI Responses API: returns Vec<json> for "input", with a stable prefix first.
pub fn build_openai_responses_input(
    system: Option<&str>,
    static_docs_prefix: Option<&str>,
    segs: &[Segment<'_>],
) -> Vec<serde_json::Value> {
    let mut v = Vec::with_capacity(
        segs.len() + if system.is_some() { 1 } else { 0 } + if static_docs_prefix.is_some() { 1 } else { 0 },
    );

    if let Some(sys) = system {
        v.push(json!({
            "role": "system",
            "content": [ { "type": "input_text", "text": sys } ]
        }));
    }

    if let Some(prefix) = static_docs_prefix {
        v.push(json!({
            "role": "user",
            "content": [ { "type": "input_text", "text": prefix } ]
        }));
    }

    for s in segs {
        v.push(json!({
            "role": s.role,
            "content": [ { "type": "input_text", "text": s.text } ]
        }));
    }
    v
}

/// Anthropic Messages API: (system, messages) with **one** user message containing block-segmented content.
pub fn build_anthropic_messages(
    system: Option<&str>,
    segs: &[Segment<'_>],
) -> (Option<serde_json::Value>, Vec<serde_json::Value>) {
    let sys = system.map(|s| serde_json::Value::String(s.to_string()));
    let blocks: Vec<_> = segs.iter().map(|s| json!({ "type": "text", "text": s.text })).collect();
    let messages = vec![json!({ "role": "user", "content": blocks })];
    (sys, messages)
}

// Provider-specific context limits
pub fn anthropic_ctx_limit_tokens(_model: &str) -> usize {
    200_000 - 2000 // extra space
}

pub fn openai_ctx_limit_tokens(model: &str) -> usize {
    let m = model.to_ascii_lowercase();
    if m.contains("gpt-5") || m.contains("gpt-4.1") {
        300_000
    } else if m.contains("gpt-4o") || m.contains("o4") {
        128_000
    } else {
        128_000
    }
}

pub fn deepseek_ctx_limit_tokens(model: &str) -> usize {
    let m = model.to_ascii_lowercase();

    if m.starts_with("deepseek-reasoner") || m.starts_with("deepseek-r1") {
        return 128_000;
    }
    if m.starts_with("deepseek-chat") || m.starts_with("deepseek-v3") {
        return 128_000;
    }

    // Fallback
    128_000
}

/// Return Gemini's context window (in tokens) for a given model.
/// - Honors env override `GEMINI_CTX_LIMIT_TOKENS`.
/// - Provides per-model fallbacks with conservative defaults.
pub fn gemini_ctx_limit_tokens(model: &str) -> usize {
    let m = model.to_ascii_lowercase();

    // Gemini 2.5 series (very large)
    if m.contains("2.5") && (m.contains("pro") || m.contains("flash")) {
        return 1_000_000;
    }

    // Gemini 1.5 Pro/Flash (large windows)
    if m.contains("1.5") {
        return 1_000_000;
    }

    // Generic gemini-*-pro/flash catch-alls
    if (m.contains("pro") || m.contains("flash")) && m.contains("gemini") {
        return 1_000_000;
    }

    // Fallback
    1_000_000
}

pub fn meta_ctx_limit_tokens(model: &str) -> usize {
    let m = model.to_ascii_lowercase();
    if m.contains("405b") || m.contains("70b") || m.contains("chat") {
        return 120_000; //8k headroom
    }
    120_000 //8k headroom
}

pub fn xai_ctx_limit_tokens(model: &str) -> usize {
    let m = model.to_ascii_lowercase();
    if m.contains("grok-4") || m.contains("grok-3") {
        return 128_000;
    }
    128_000
}

/// Desired output tokens (planning only). Env override: `LLM_DESIRED_OUTPUT_TOKENS` (usize).
pub fn desired_output_tokens() -> usize {
    std::env::var("LLM_DESIRED_OUTPUT_TOKENS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1500)
}

/// Static headroom from env, with sensible defaults and 500-token rounding.
/// Precedence (first found wins):
///   1) Per-vendor: {OPENAI|ANTHROPIC|GOOGLE|XAI|DEEPSEEK|META}_HEADROOM_TOKENS
///   2) Global: LLM_HEADROOM_TOKENS
///   3) Default per vendor (see `default_headroom`)
pub fn headroom_tokens_env(vendor: Vendor) -> usize {
    // Per-vendor env
    let per_vendor = match vendor {
        Vendor::OpenAi => std::env::var("OPENAI_HEADROOM_TOKENS").ok(),
        Vendor::Anthropic => std::env::var("ANTHROPIC_HEADROOM_TOKENS").ok(),
        Vendor::Google => std::env::var("GOOGLE_HEADROOM_TOKENS").ok(),
        Vendor::Xai => std::env::var("XAI_HEADROOM_TOKENS").ok(),
        Vendor::DeepSeek => std::env::var("DEEPSEEK_HEADROOM_TOKENS").ok(),
        Vendor::Meta => std::env::var("META_HEADROOM_TOKENS").ok(),
    };

    let raw = per_vendor
        .or_else(|| std::env::var("LLM_HEADROOM_TOKENS").ok())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| default_headroom(vendor));

    // Clamp to a minimum (avoid absurdly small values) and round to 500 for stability.
    let clamped = raw.max(500);
    ((clamped + 499) / 500) * 500
}

fn default_headroom(vendor: Vendor) -> usize {
    match vendor {
        // Conservative floors tuned for typical max-context models
        Vendor::OpenAi | Vendor::Xai | Vendor::DeepSeek | Vendor::Meta => 3_000,
        Vendor::Anthropic => 5_000,
        Vendor::Google => 10_000,
    }
}

/// Tail-trim `prefix` to `allowance_tokens` deterministically.
/// Does NOT look at system/segments; stable bytes across calls.
pub fn deterministic_trim_prefix(prefix: &str, allowance_tokens: usize) -> String {
    if prefix.is_empty() {
        return String::new();
    }
    if estimate_tokens(prefix) <= allowance_tokens {
        return prefix.to_string();
    }

    // binary search on char boundary to approx token allowance
    let chars: Vec<char> = prefix.chars().collect();
    let mut lo = 0usize;
    let mut hi = chars.len();
    // we keep the HEAD, drop the tail (cache-friendly)
    while lo < hi {
        let mid = (lo + hi) / 2;
        let candidate: String = chars[..mid].iter().collect();
        if estimate_tokens(&candidate) <= allowance_tokens {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let keep = lo.saturating_sub(1);
    chars[..keep].iter().collect()
}

/// Static reserve for "non-context" content:
/// (system + task/instructions + model output).
/// Env precedence:
///   1) {OPENAI|ANTHROPIC|GOOGLE|XAI|DEEPSEEK|META}_RESERVE_TOKENS
///   2) LLM_RESERVE_TOKENS
///   3) vendor defaults below
pub fn non_context_reserve_tokens_env(vendor: Vendor) -> usize {
    // per-vendor override
    let key = match vendor {
        Vendor::OpenAi => "OPENAI_RESERVE_TOKENS",
        Vendor::Anthropic => "ANTHROPIC_RESERVE_TOKENS",
        Vendor::Google => "GOOGLE_RESERVE_TOKENS",
        Vendor::Xai => "XAI_RESERVE_TOKENS",
        Vendor::DeepSeek => "DEEPSEEK_RESERVE_TOKENS",
        Vendor::Meta => "META_RESERVE_TOKENS",
    };
    if let Ok(v) = std::env::var(key) {
        if let Ok(n) = v.parse::<usize>() {
            return round_500(n.max(2_000));
        }
    }
    // global override
    if let Ok(v) = std::env::var("LLM_RESERVE_TOKENS") {
        if let Ok(n) = v.parse::<usize>() {
            return round_500(n.max(2_000));
        }
    }

    // defaults (conservative)
    let def = match vendor {
        Vendor::OpenAi => 12_000, // room for task + output
        Vendor::Anthropic => 12_000,
        Vendor::Google => 16_000, // gemini often bigger outputs
        Vendor::Xai => 12_000,
        Vendor::DeepSeek => 12_000,
        Vendor::Meta => 12_000,
    };
    round_500(def)
}

fn round_500(n: usize) -> usize {
    (n + 499) / 500 * 500
}
