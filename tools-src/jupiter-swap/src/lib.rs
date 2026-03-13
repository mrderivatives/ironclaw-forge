wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../../wit/tool.wit",
});

use exports::near::agent::tool::Guest;

struct Tool;

impl Guest for Tool {
    fn execute(req: exports::near::agent::tool::Request) -> exports::near::agent::tool::Response {
        match run(&req.params) {
            Ok(out) => exports::near::agent::tool::Response { output: Some(out), error: None },
            Err(e)  => exports::near::agent::tool::Response { output: None, error: Some(e) },
        }
    }

    fn schema() -> String {
        r#"{"type":"object","required":["from","to","amount","taker","confirmed"],"properties":{"from":{"type":"string","description":"Input token symbol or mint address (e.g. 'SOL', 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v' for USDC)"},"to":{"type":"string","description":"Output token symbol or mint address"},"amount":{"type":"number","description":"Amount in human units (e.g. 0.01 for 0.01 SOL, 10 for 10 USDC)"},"taker":{"type":"string","description":"Signer wallet address (your wallet pubkey)"},"confirmed":{"type":"boolean","description":"Must be true to execute — prevents accidental swaps. Show the quote first and ask user to confirm."}}}"#.into()
    }

    fn description() -> String {
        "Execute a token swap on Solana via Jupiter Ultra. Gets a quote, signs, and submits the transaction. Always show the quote and ask for user confirmation before setting confirmed:true.".into()
    }
}

/// Token mint lookup for common symbols.
fn resolve_mint(token: &str) -> &str {
    match token.to_uppercase().as_str() {
        "SOL"  => "So11111111111111111111111111111111111111112",
        "USDC" => "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "USDT" => "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
        "BONK" => "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263",
        "JUP"  => "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN",
        "WIF"  => "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm",
        "RAY"  => "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R",
        _      => token, // assume it's already a mint address
    }
}

/// Decimals for amount → smallest-unit conversion.
fn decimals_for(token: &str) -> u32 {
    match token.to_uppercase().as_str() {
        "SOL"  => 9,
        "USDC" | "USDT" => 6,
        "BONK" => 5,
        _      => 9, // default; Jupiter will handle errors for unknown mints
    }
}

// ── Solana transaction signing helpers ─────────────────────────────────────

/// Read a compact-u16 from `bytes` at position 0.
/// Returns (value, bytes_consumed).
fn read_compact_u16(bytes: &[u8]) -> Result<(u16, usize), String> {
    if bytes.is_empty() {
        return Err("empty buffer for compact-u16".into());
    }
    let b0 = bytes[0] as u16;
    if b0 < 128 {
        return Ok((b0, 1));
    }
    if bytes.len() < 2 {
        return Err("truncated compact-u16".into());
    }
    let b1 = bytes[1] as u16;
    if b1 < 128 {
        return Ok(((b1 << 7) | (b0 & 0x7f), 2));
    }
    if bytes.len() < 3 {
        return Err("truncated 3-byte compact-u16".into());
    }
    let b2 = bytes[2] as u16;
    Ok(((b2 << 14) | ((b1 & 0x7f) << 7) | (b0 & 0x7f), 3))
}

/// Extract the message bytes from an unsigned Solana versioned transaction.
///
/// Layout: compact_u16(num_sigs) || sigs[num_sigs * 64] || message_bytes
fn message_bytes(tx: &[u8]) -> Result<&[u8], String> {
    let (num_sigs, hdr_len) = read_compact_u16(tx)?;
    let sigs_end = hdr_len + (num_sigs as usize) * 64;
    if tx.len() <= sigs_end {
        return Err(format!(
            "tx too short: len={} sigs_end={}", tx.len(), sigs_end
        ));
    }
    Ok(&tx[sigs_end..])
}

/// Inject a 64-byte ed25519 signature into the first slot of a Solana transaction
/// (replacing the all-zeros placeholder) and return the modified bytes.
fn inject_signature(tx: &[u8], sig: &[u8]) -> Result<Vec<u8>, String> {
    if sig.len() != 64 {
        return Err(format!("signature must be 64 bytes, got {}", sig.len()));
    }
    let (num_sigs, hdr_len) = read_compact_u16(tx)?;
    if num_sigs == 0 {
        return Err("transaction has 0 signature slots".into());
    }
    if tx.len() < hdr_len + 64 {
        return Err("transaction too short to hold a signature".into());
    }
    let mut signed = tx.to_vec();
    signed[hdr_len..hdr_len + 64].copy_from_slice(sig);
    Ok(signed)
}

// ── Main logic ──────────────────────────────────────────────────────────────

fn run(params_json: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Params {
        from: String,
        to: String,
        amount: f64,
        taker: String,
        confirmed: bool,
    }

    let p: Params = serde_json::from_str(params_json).map_err(|e| e.to_string())?;

    if !p.confirmed {
        return Err("Set confirmed:true to execute. Show the user the quote first and ask them to confirm.".into());
    }

    let input_mint  = resolve_mint(&p.from);
    let output_mint = resolve_mint(&p.to);
    let decimals    = decimals_for(&p.from);
    let amount_raw  = (p.amount * 10f64.powi(decimals as i32)) as u64;

    // ── Step 1: Get Jupiter Ultra order ──────────────────────────────────
    let order_url = format!(
        "https://api.jup.ag/ultra/v1/order?inputMint={}&outputMint={}&amount={}&taker={}",
        input_mint, output_mint, amount_raw, p.taker
    );

    let order_resp = near::agent::host::http_request("GET", &order_url, "{}", None, None)
        .map_err(|e| format!("Jupiter order failed: {e}"))?;

    let order: serde_json::Value = serde_json::from_slice(&order_resp.body)
        .map_err(|e| format!("Jupiter order parse failed: {e}"))?;

    if let Some(err) = order.get("error") {
        return Err(format!("Jupiter order error: {}", err));
    }

    let request_id = order["requestId"]
        .as_str()
        .ok_or("missing requestId in order")?
        .to_string();

    let tx_b64 = order["transaction"]
        .as_str()
        .ok_or("missing transaction in order")?;

    // ── Step 2: Sign the transaction ─────────────────────────────────────
    // Decode base64 transaction
    let tx_bytes = base64_decode(tx_b64)
        .map_err(|e| format!("base64 decode failed: {e}"))?;

    // Extract the message bytes (what gets signed)
    let msg = message_bytes(&tx_bytes)
        .map_err(|e| format!("message extraction failed: {e}"))?;

    // Call sign_bytes host function — key never leaves the host process
    let signature = near::agent::host::sign_bytes("solana_private_key", msg)
        .map_err(|e| format!("signing failed: {e}. Is solana_private_key provisioned?"))?;

    // Inject signature into transaction
    let signed_tx = inject_signature(&tx_bytes, &signature)
        .map_err(|e| format!("signature injection failed: {e}"))?;

    let signed_tx_b64 = base64_encode(&signed_tx);

    // ── Step 3: Execute on Jupiter Ultra ─────────────────────────────────
    let execute_body = serde_json::json!({
        "requestId": request_id,
        "signedTransaction": signed_tx_b64,
    })
    .to_string();

    let exec_resp = near::agent::host::http_request(
        "POST",
        "https://api.jup.ag/ultra/v1/execute",
        &execute_body,
        Some(&[("Content-Type".to_string(), "application/json".to_string())]),
        None,
    )
    .map_err(|e| format!("Jupiter execute failed: {e}"))?;

    let result: serde_json::Value = serde_json::from_slice(&exec_resp.body)
        .map_err(|e| format!("Jupiter execute parse failed: {e}"))?;

    if let Some(err) = result.get("error") {
        return Err(format!("Jupiter execute error: {}", err));
    }

    // ── Step 4: Return result ─────────────────────────────────────────────
    let signature_str = result["signature"]
        .as_str()
        .unwrap_or("unknown");

    Ok(serde_json::json!({
        "status": "executed",
        "signature": signature_str,
        "solscan": format!("https://solscan.io/tx/{}", signature_str),
        "inputMint": input_mint,
        "outputMint": output_mint,
        "inAmount": order["inAmount"],
        "outAmount": order["outAmount"],
        "priceImpactPct": order["priceImpactPct"],
    })
    .to_string())
}

// ── Minimal base64 (no external dep beyond what's in Cargo.toml) ────────────

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
        .map_err(|e| e.to_string())
}

fn base64_encode(bytes: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

export!(Tool);
