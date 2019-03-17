fn main() {
    env_logger::init();

    let r = lynx::get("https://statsapi.web.nhl.com/api/v1/schedule?expand=schedule.linescore");

    let (status, headers, reader) = r.send().unwrap();
    println!("{:?} {:#?}", status, headers);
    println!("{}", reader.string().unwrap());
}
