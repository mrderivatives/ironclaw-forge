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
        r#"{"type":"object","required":["tokens"],"properties":{"tokens":{"type":"array","items":{"type":"string"},"description":"CoinGecko token IDs (e.g. solana, usd-coin, bonk)"}}}"#.into()
    }
    fn description() -> String {
        "Get current USD prices for crypto tokens via CoinGecko. Use token IDs like: solana, usd-coin, bonk, ethereum.".into()
    }
}

fn run(params: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Params { tokens: Vec<String> }
    let p: Params = serde_json::from_str(params).map_err(|e| e.to_string())?;
    if p.tokens.is_empty() { return Err("tokens list cannot be empty".into()); }

    let ids = p.tokens.join(",");
    let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&include_24hr_change=true", ids);

    let resp = near::agent::host::http_request("GET", &url, "{}", None, Some(15000))
        .map_err(|e| e)?;

    Ok(String::from_utf8_lossy(&resp.body).to_string())
}

export!(Tool);
