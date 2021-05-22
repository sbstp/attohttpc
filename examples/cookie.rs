
fn main() -> attohttpc::Result {
    env_logger::init();

    let resp = attohttpc::get("https://httpbin.org/get")
        .cookie(attohttpc::Cookie::new("name", "value"))
        .send()?;

    println!("Status: {:?}", resp.status());
    println!("Headers:\n{:#?}", resp.headers());
    println!("Body:\n{}", resp.text()?);

    Ok(())
}
