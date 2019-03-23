use attohttpc::HttpResult;

fn main() -> HttpResult {
    env_logger::init();

    let (status, headers, reader) = attohttpc::post("https://httpbin.org/post")
        .text("hello, world!")
        .send()?;
    println!("Status: {:?}", status);
    println!("Headers:\n{:#?}", headers);
    println!("Body:\n{}", reader.text()?);

    Ok(())
}
