wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../../wit/tool.wit",
});

use exports::near::agent::tool::Guest;
use ed25519_dalek::{SigningKey, Signer};

struct Tool;

impl Guest for Tool {
    fn execute(req: exports::near::agent::tool::Request) -> exports::near::agent::tool::Response {
        match run(&req.params) {
            Ok(out) => exports::near::agent::tool::Response { output: Some(out), error: None },
            Err(e)  => exports::near::agent::tool::Response { output: None, error: Some(e) },
        }
    }
    fn schema() -> String {
        r#"{"type":"object","required":["from","to","amount","confirmed"],"properties":{"from":{"type":"string"},"to":{"type":"string"},"amount":{"type":"number","description":"Amount in human units"},"confirmed":{"type":"boolean","description":"Must be true to execute — prevents accidental swaps"}}}"#.into()
    }
    fn description() -> String {
        "Execute a token swap via Jupiter Ultra. Signs and submits the transaction on-chain. Set confirmed:true to proceed.".into()
    }
}

fn run(params: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Params { from: String, to: String, amount: f64, confirmed: bool }
    let p: Params = serde_json::from_str(params).map_err(|e| e.to_string())?;
    if !p.confirmed { return Err("Set confirmed: true to execute this swap.".into()); }

    // Read the private key from agent secrets
    let sk_b58 = near::agent::secrets::get("solana_private_key").ok_or("secret not found")?;
    let raw = bs58::decode(&sk_b58).into_vec().map_err(|e| e.to_string())?;
    if raw.len() < 64 { return Err("invalid key length".into()); }
    let seed: [u8; 32] = raw[..32].try_into().unwrap();
    let signing_key = SigningKey::from_bytes(&seed);
    let taker = bs58::encode(signing_key.verifying_key().to_bytes()).into_string();

    let lamports = (p.amount * 1_000_000_000.0) as u64;
    let order_url = format!(
        "https://api.jup.ag/ultra/v1/order?inputMint={}&outputMint={}&amount={}&taker={}",
        p.from, p.to, lamports, taker
    );
    let order_resp = near::agent::host::http_request(
        "GET",
        &order_url,
        "{}",
        None,
        None,
    ).map_err(|e| e)?;
    let order: serde_json::Value = serde_json::from_slice(&order_resp.body).map_err(|e| e.to_string())?;
    let tx_b64 = order["transaction"].as_str().ok_or("no transaction in order")?;
    let request_id = order["requestId"].as_str().unwrap_or("").to_string();

    // Decode transaction, sign, re-encode
    let mut tx_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, tx_b64)
        .map_err(|e| e.to_string())?;
    // Solana versioned tx: byte 0 = 0x80 (version prefix), bytes 1..65 = first signature slot
    if tx_bytes.len() < 65 { return Err("tx too short".into()); }
    let sig = signing_key.sign(&tx_bytes);
    tx_bytes[1..65].copy_from_slice(&sig.to_bytes());
    let signed_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &tx_bytes);

    let execute_body = serde_json::json!({ "signedTransaction": signed_b64, "requestId": request_id });
    let headers_json = r#"{"Content-Type":"application/json"}"#;
    let exec_resp = near::agent::host::http_request(
        "POST",
        "https://api.jup.ag/ultra/v1/execute",
        headers_json,
        Some(execute_body.to_string().as_bytes()),
        None,
    ).map_err(|e| e)?;

    let result: serde_json::Value = serde_json::from_slice(&exec_resp.body).map_err(|e| e.to_string())?;
    let sig_str = result["signature"].as_str().unwrap_or("unknown");
    Ok(serde_json::json!({
        "signature": sig_str,
        "explorer": format!("https://solscan.io/tx/{}", sig_str),
        "status": result["status"],
    }).to_string())
}

export!(Tool);
