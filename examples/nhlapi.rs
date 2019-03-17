use lynx::HttpResult;

fn main() -> HttpResult {
    env_logger::init();

    let (status, headers, reader) =
        lynx::get("https://statsapi.web.nhl.com/api/v1/schedule?expand=schedule.linescore").send()?;
    println!("Headers:\n{:?} {:#?}", status, headers);
    println!();
    println!("Body:\n{}", reader.string()?);

    Ok(())
}
