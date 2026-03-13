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
        r#"{"type":"object","required":["from","to","amount","taker","confirmed"],"properties":{"from":{"type":"string","description":"Input token symbol or mint address"},"to":{"type":"string","description":"Output token symbol or mint address"},"amount":{"type":"number","description":"Amount in human units (e.g. 0.01 for 0.01 SOL)"},"taker":{"type":"string","description":"Signer wallet address (your wallet pubkey)"},"confirmed":{"type":"boolean","description":"Must be true to proceed — prevents accidental swaps"}}}"#.into()
    }

    fn description() -> String {
        "Prepare a Jupiter Ultra swap transaction. Returns the order details and unsigned transaction. Set confirmed:true to proceed. NOTE: on-chain signing is pending a host WIT extension — this tool currently returns the prepared order for review.".into()
    }
}

fn run(params: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Params {
        from: String,
        to: String,
        amount: f64,
        taker: String,
        confirmed: bool,
    }

    let p: Params = serde_json::from_str(params).map_err(|e| e.to_string())?;
    if !p.confirmed {
        return Err("Set confirmed: true to prepare this swap.".into());
    }

    // Jupiter Ultra uses lamports (1 SOL = 1_000_000_000 lamports)
    let lamports = (p.amount * 1_000_000_000.0) as u64;
    let url = format!(
        "https://api.jup.ag/ultra/v1/order?inputMint={}&outputMint={}&amount={}&taker={}",
        p.from, p.to, lamports, p.taker
    );

    let resp = near::agent::host::http_request("GET", &url, "{}", None, None)
        .map_err(|e| e)?;

    let order: serde_json::Value =
        serde_json::from_slice(&resp.body).map_err(|e| e.to_string())?;

    // Return order details without executing — signing requires host WIT extension
    Ok(serde_json::json!({
        "status": "prepared",
        "inputMint": order["inputMint"],
        "outputMint": order["outputMint"],
        "inAmount": order["inAmount"],
        "outAmount": order["outAmount"],
        "priceImpactPct": order["priceImpactPct"],
        "requestId": order["requestId"],
        "note": "On-chain execution pending sign-bytes WIT extension. Use jupiter-quote to preview swaps."
    })
    .to_string())
}

export!(Tool);
