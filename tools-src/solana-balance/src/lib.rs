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
        r#"{"type":"object","properties":{"wallet":{"type":"string","description":"Solana wallet address (required)"}},"required":["wallet"]}"#.into()
    }
    fn description() -> String {
        "Get SOL balance for a Solana wallet address using the public Solana RPC.".into()
    }
}

fn run(params: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Params { wallet: String }
    let p: Params = serde_json::from_str(params).map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getBalance",
        "params": [p.wallet]
    });

    let resp = near::agent::host::http_request(
        "POST",
        "https://api.mainnet-beta.solana.com",
        r#"{"Content-Type":"application/json"}"#,
        Some(body.to_string().into_bytes()).as_deref(),
        Some(15000),
    ).map_err(|e| e)?;

    let result: serde_json::Value = serde_json::from_slice(&resp.body).map_err(|e| e.to_string())?;
    let lamports = result["result"]["value"].as_u64().unwrap_or(0);
    let sol = lamports as f64 / 1_000_000_000.0;

    Ok(serde_json::json!({
        "wallet": p.wallet,
        "sol": sol,
        "lamports": lamports
    }).to_string())
}

export!(Tool);
