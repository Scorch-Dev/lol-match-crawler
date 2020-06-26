
error_chain!{

    links {
        LolApi(crate::lol_api::Error, crate::lol_api::ErrorKind);
    }

    foreign_links {
        Io(::tokio::io::Error);
        JoinError(::tokio::task::JoinError);
    }

}