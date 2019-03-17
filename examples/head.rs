fn main() {
    env_logger::init();

    let r = lynx::head("http://httpbin.org");

    let (status, headers, reader) = r.send().unwrap();
    println!("{:?} {:#?}", status, headers);
}
