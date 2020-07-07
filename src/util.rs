
/// Loads the api key from a file called key.txt located
/// in the project root folder.
pub fn get_key() -> String {
    std::fs::read_to_string("./key.txt")
        .expect("Can't open file <project root>/key.txt. Please put the riot api key in this file.")
        .trim().to_string()
}
