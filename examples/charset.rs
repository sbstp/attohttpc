fn main() -> Result<(), attohttpc::Error> {
    env_logger::init();

    let resp = attohttpc::get("https://rust-lang.org/").send()?;
    println!("{}", resp.into_body().text()?);
    Ok(())
}
