/// returns tuple (FILE_NAME, EXTENSION)
pub fn get_file_meta(file_name: &str) -> (Option<String>, Option<String>) {
    let split = file_name.split('.');
    let name_vec = split.collect::<Vec<&str>>();
    let len = name_vec.len();
    if len < 2 {
        return (None, None);
    }
    let file_name = name_vec[..len - 1].join("");
    let extension = name_vec[len - 1];
    (Some(file_name), Some(extension.to_owned()))
}
