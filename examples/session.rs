use attohttpc::{Result, Session};

fn main() -> Result {
    let mut sess = Session::new().header("Authorization", "Bearer big bear");

    let resp = sess.get("https://httpbin.org/get").send()?;
    println!("{}", resp.text()?);

    Ok(())
}
