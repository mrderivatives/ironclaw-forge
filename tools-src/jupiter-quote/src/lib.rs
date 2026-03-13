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
        r#"{"type":"object","required":["inputMint","outputMint","amount"],"properties":{"inputMint":{"type":"string","description":"Input token mint (e.g. So11111111111111111111111111111111111111112 for SOL)"},"outputMint":{"type":"string","description":"Output token mint (e.g. EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v for USDC)"},"amount":{"type":"integer","description":"Amount in smallest unit (lamports for SOL, 1 SOL = 1000000000)"},"slippageBps":{"type":"integer","description":"Slippage in basis points (default 50 = 0.5%)","default":50}}}"#.into()
    }
    fn description() -> String {
        "Get a swap quote from Jupiter. Returns expected output amount and route. Does NOT execute the swap. Common mints: SOL=So11111111111111111111111111111111111111112, USDC=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v, BONK=DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263, NEAR(wormhole)=3ZLekZYq2qkZiSpnSvabjit34tUkjSwD1JFuW9as9wBG".into()
    }
}

fn run(params: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Params {
        #[serde(rename = "inputMint")] input_mint: String,
        #[serde(rename = "outputMint")] output_mint: String,
        amount: u64,
        #[serde(rename = "slippageBps", default = "default_slippage")] slippage_bps: u64,
    }
    fn default_slippage() -> u64 { 50 }

    let p: Params = serde_json::from_str(params).map_err(|e| e.to_string())?;
    let url = format!(
        "https://lite-api.jup.ag/swap/v1/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}",
        p.input_mint, p.output_mint, p.amount, p.slippage_bps
    );

    let resp = near::agent::host::http_request("GET", &url, "{}", None, Some(15000))
        .map_err(|e| e)?;

    let data: serde_json::Value = serde_json::from_slice(&resp.body).map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "inputMint": data["inputMint"],
        "outputMint": data["outputMint"],
        "inAmount": data["inAmount"],
        "outAmount": data["outAmount"],
        "priceImpactPct": data["priceImpactPct"],
        "swapUsdValue": data["swapUsdValue"],
        "_quoteResponse": data  // full response needed for swap execution
    }).to_string())
}

export!(Tool);
