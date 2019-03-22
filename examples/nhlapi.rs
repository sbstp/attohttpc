use lynx::HttpResult;

fn main() -> HttpResult {
    env_logger::init();

    let (status, headers, reader) = lynx::get("https://statsapi.web.nhl.com/api/v1/schedule").send()?;
    println!("Status: {:?}", status);
    println!("Headers:\n{:#?}", headers);
    println!("Body:\n{}", reader.text()?);

    Ok(())
}
