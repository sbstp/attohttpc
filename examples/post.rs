use lynx::HttpResult;

fn main() -> HttpResult {
    env_logger::init();

    let (status, headers, reader) = lynx::post("https://httpbin.org/post").body("hello, world!").send()?;
    println!("Headers:\n{:?} {:#?}", status, headers);
    println!();
    println!("Body:\n{}", reader.string()?);

    Ok(())
}
