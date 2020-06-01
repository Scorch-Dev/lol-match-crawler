
error_chain!{

    foreign_links {
        Reqwest(::reqwest::Error);
        Serde(::serde::de::value::Error);
    }

    errors {
        Query(query : String) {
            description("Query to LoL API failed.")
            display("query to {:?} failed.", query)
        }
        Http(response : reqwest::blocking::Response) {
            description("LoL API did not respond 200 OK.")
            display("The LoL API replied with a failing HTTP status code, {:?}.", response.status().to_string())
        }
    }
}

impl From<reqwest::blocking::Response> for ErrorKind {
    fn from(r : reqwest::blocking::Response) -> Self {
        ErrorKind::Http(r)
    }
}