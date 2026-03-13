wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
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
        r#"{"type":"object","required":["inputMint","outputMint","amount","userPublicKey","confirmed"],"properties":{"inputMint":{"type":"string","description":"Input token mint address"},"outputMint":{"type":"string","description":"Output token mint address"},"amount":{"type":"integer","description":"Amount in lamports (1 SOL = 1000000000)"},"userPublicKey":{"type":"string","description":"Agent wallet public key for signing"},"slippageBps":{"type":"integer","default":50},"confirmed":{"type":"boolean","description":"Must be true to execute — safety guard"}}}"#.into()
    }
    fn description() -> String {
        "Execute a real token swap via Jupiter. Gets a quote, signs the transaction using the agent's keypair, and submits to Solana. Set confirmed:true. Common mints: SOL=So11111111111111111111111111111111111111112, USDC=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".into()
    }
}

fn run(params: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Params {
        #[serde(rename = "inputMint")] input_mint: String,
        #[serde(rename = "outputMint")] output_mint: String,
        amount: u64,
        #[serde(rename = "userPublicKey")] user_public_key: String,
        #[serde(rename = "slippageBps", default = "default_slippage")] slippage_bps: u64,
        confirmed: bool,
    }
    fn default_slippage() -> u64 { 50 }

    let p: Params = serde_json::from_str(params).map_err(|e| e.to_string())?;
    if !p.confirmed {
        return Err("Set confirmed:true to execute the swap. Show the quote first and ask user to confirm.".into());
    }

    // Step 1: Get quote
    let quote_url = format!(
        "https://lite-api.jup.ag/swap/v1/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}",
        p.input_mint, p.output_mint, p.amount, p.slippage_bps
    );
    let quote_resp = near::agent::host::http_request("GET", &quote_url, "{}", None, Some(15000))
        .map_err(|e| format!("quote failed: {e}"))?;
    let quote: serde_json::Value = serde_json::from_slice(&quote_resp.body)
        .map_err(|e| format!("quote parse failed: {e}"))?;

    if quote.get("error").is_some() {
        return Err(format!("Quote error: {}", quote["error"]));
    }

    // Step 2: Build swap transaction
    let swap_body = serde_json::json!({
        "quoteResponse": quote,
        "userPublicKey": p.user_public_key,
        "dynamicComputeUnitLimit": true,
        "prioritizationFeeLamports": "auto"
    });
    let swap_resp = near::agent::host::http_request(
        "POST",
        "https://lite-api.jup.ag/swap/v1/swap",
        r#"{"Content-Type":"application/json"}"#,
        Some(swap_body.to_string().as_bytes()),
        Some(30000),
    ).map_err(|e| format!("swap build failed: {e}"))?;

    let swap_data: serde_json::Value = serde_json::from_slice(&swap_resp.body)
        .map_err(|e| format!("swap parse failed: {e}"))?;

    if swap_data.get("error").is_some() {
        return Err(format!("Swap build error: {}", swap_data["error"]));
    }

    let tx_b64 = swap_data["swapTransaction"].as_str()
        .ok_or("no swapTransaction in response")?;

    // Step 3: Sign transaction using host sign-bytes primitive
    let tx_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, tx_b64)
        .map_err(|e| format!("tx decode failed: {e}"))?;

    let sig_bytes = near::agent::host::sign_bytes("solana_private_key", &tx_bytes)
        .map_err(|e| format!("signing failed: {e}"))?;

    if sig_bytes.len() != 64 {
        return Err(format!("unexpected signature length: {}", sig_bytes.len()));
    }

    // Insert signature into transaction (position [1..65] for versioned tx)
    let mut signed_tx = tx_bytes.clone();
    signed_tx[1..65].copy_from_slice(&sig_bytes);
    let signed_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &signed_tx);

    // Step 4: Send signed transaction via public Solana RPC
    let send_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [signed_b64, {"encoding": "base64", "skipPreflight": false}]
    });
    let send_resp = near::agent::host::http_request(
        "POST",
        "https://api.mainnet-beta.solana.com",
        r#"{"Content-Type":"application/json"}"#,
        Some(send_body.to_string().as_bytes()),
        Some(30000),
    ).map_err(|e| format!("send failed: {e}"))?;

    let send_data: serde_json::Value = serde_json::from_slice(&send_resp.body)
        .map_err(|e| format!("send parse failed: {e}"))?;

    if let Some(err) = send_data.get("error") {
        return Err(format!("Transaction failed: {}", err));
    }

    let signature = send_data["result"].as_str().unwrap_or("unknown");
    Ok(serde_json::json!({
        "signature": signature,
        "explorer": format!("https://solscan.io/tx/{}", signature),
        "inAmount": quote["inAmount"],
        "outAmount": quote["outAmount"],
        "inputMint": p.input_mint,
        "outputMint": p.output_mint
    }).to_string())
}

export!(Tool);
