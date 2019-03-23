fn main() -> attohttpc::Result {
    env_logger::init();

    let (status, headers, reader) = attohttpc::get("https://statsapi.web.nhl.com/api/v1/schedule").send()?;
    println!("Status: {:?}", status);
    println!("Headers:\n{:#?}", headers);
    println!("Body:\n{}", reader.text()?);

    Ok(())
}
