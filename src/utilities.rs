pub fn progress_formatted(seconds: i32) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!("{:02}:{:02}", minutes, seconds)
}
