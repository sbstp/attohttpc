use serde_json::json;

use lynx::HttpResult;

fn main() -> HttpResult {
    env_logger::init();

    let body = json!({
        "hello": "world",
    });

    let (status, headers, reader) = lynx::post("http://httpbin.org/post").json(&body)?.send()?;
    println!("Headers:\n{:?} {:#?}", status, headers);
    println!();
    println!("Body:\n{}", reader.string()?);

    Ok(())
}
