fn main() {
    env_logger::init();

    let mut r = lynx::post("https://httpbin.org/post");
    r.body("Hello world!");

    let (status, headers, reader) = r.send().unwrap();
    println!("{:?} {:#?}", status, headers);
    println!("{}", reader.string().unwrap());
}
