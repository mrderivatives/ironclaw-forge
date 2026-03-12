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
        r#"{"type":"object","required":["from","to","amount","taker"],"properties":{"from":{"type":"string","description":"Input token symbol or mint"},"to":{"type":"string","description":"Output token symbol or mint"},"amount":{"type":"number","description":"Amount in human units (e.g. 0.01 for 0.01 SOL)"},"taker":{"type":"string","description":"Taker wallet address"}}}"#.into()
    }
    fn description() -> String {
        "Get a swap quote from Jupiter Ultra. Shows expected output, price impact, and rate — does NOT execute the swap.".into()
    }
}

fn run(params: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Params { from: String, to: String, amount: f64, taker: String }
    let p: Params = serde_json::from_str(params).map_err(|e| e.to_string())?;

    // SOL = 9 decimals, USDC = 6; use 9 as default
    let lamports = (p.amount * 1_000_000_000.0) as u64;
    let url = format!(
        "https://api.jup.ag/ultra/v1/order?inputMint={}&outputMint={}&amount={}&taker={}",
        p.from, p.to, lamports, p.taker
    );
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
