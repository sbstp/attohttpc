use colored::*;
use serde::{Deserialize, Serialize};

type Listing = ProxyData<Posts>;

#[derive(Debug, Deserialize, Serialize)]
struct ProxyData<T> {
    pub kind: String,
    pub data: T,
}

#[derive(Debug, Deserialize, Serialize)]
struct Posts {
    pub children: Vec<ProxyData<Post>>,
    pub after: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Post {
    pub is_self: bool,
    pub url: String,
    pub subreddit: String,
}

fn create_req(url: &str) -> attohttpc::RequestBuilder {
    attohttpc::get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 6.3; Win64; x64; rv:71.0) Gecko/20100101 Firefox/71.0",
        )
        .header("Accept", "*/*")
}

fn process_items(listing: &Listing) {
    for item in &listing.data.children {
        if !item.data.is_self {
            match create_req(&item.data.url).send() {
                Ok(resp) => {
                    if resp.is_success() {
                        println!("{} from {}", resp.status(), item.data.url);
                    } else {
                        eprintln!("{}", format!("{} from {}", resp.status(), item.data.url,).red());
                    }
                }
                Err(err) => {
                    eprintln!("{}", format!("{} error from {}", err, item.data.url).red());
                }
            }
        }
    }
}

fn main() -> attohttpc::Result<()> {
    let mut listing: Listing = create_req("https://www.reddit.com/r/todayilearned/new.json")
        .send()?
        .json()?;

    while let Some(after) = &listing.data.after {
        if listing.data.children.is_empty() {
            break;
        }

        process_items(&listing);

        listing = create_req("https://www.reddit.com/r/todayilearned/new.json")
            .param("after", &after)
            .send()?
            .json()?;
    }

    Ok(())
}
