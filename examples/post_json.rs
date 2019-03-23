use serde_json::json;

fn main() -> attohttpc::Result {
    env_logger::init();

    let body = json!({
        "hello": "world",
    });

    let (status, headers, reader) = attohttpc::post("http://httpbin.org/post").json(&body)?.send()?;
    println!("Status: {:?}", status);
    println!("Headers:\n{:#?}", headers);
    println!("Body:\n{}", reader.text_utf8()?);

    Ok(())
}
