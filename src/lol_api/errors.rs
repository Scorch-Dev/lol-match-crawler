
error_chain!{

    foreign_links {
        Reqwest(::reqwest::Error);
        HeaderToString(::reqwest::header::ToStrError);
        Serde(::serde::de::value::Error);
    }

    errors {
        EndpointNotReady(status : crate::lol_api::endpoint::Status) {
            description("Endpoint is not in a ready state.")
            display("Endpoint in state {:?} is not ready to receive queries.", status)
        }
    }
}

impl From<crate::lol_api::endpoint::Status> for ErrorKind {
    fn from(status : crate::lol_api::endpoint::Status) -> Self {
        ErrorKind::EndpointNotReady(status)
    }
}