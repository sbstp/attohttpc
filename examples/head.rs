use attohttpc::HttpResult;

fn main() -> HttpResult {
    env_logger::init();

    let (status, headers, _) = attohttpc::head("http://httpbin.org").send()?;
    println!("Status: {:?}", status);
    println!("Headers:\n{:#?}", headers);

    Ok(())
}
