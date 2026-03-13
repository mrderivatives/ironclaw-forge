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
        r#"{"type":"object","properties":{"wallet":{"type":"string","description":"Solana wallet address (optional, defaults to agent wallet)"}}}"#.into()
    }
    fn description() -> String {
        "Get SOL and SPL token balances for the agent wallet (or any Solana address) via Jupiter Ultra.".into()
    }
}

fn run(params: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Params { wallet: Option<String> }
    let p: Params = serde_json::from_str(params).unwrap_or(Params { wallet: None });

    let pubkey = match p.wallet {
        Some(w) => w,
        None => return Err("wallet address is required (agent wallet injection not yet supported)".into()),
    };

    let url = format!("https://api.jup.ag/ultra/v1/balances?wallet={}", pubkey);
    let resp = near::agent::host::http_request(
        "GET",
        &url,
        "{}",
        None,
        None,
    ).map_err(|e| e)?;

    Ok(String::from_utf8_lossy(&resp.body).to_string())
}

export!(Tool);
