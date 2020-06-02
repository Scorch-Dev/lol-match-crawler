
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
    }
}