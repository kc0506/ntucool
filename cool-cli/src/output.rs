pub enum OutputFormat {
    Table,
    Json,
}

impl OutputFormat {
    pub fn from_flag(json: bool) -> Self {
        if json {
            OutputFormat::Json
        } else {
            OutputFormat::Table
        }
    }
}
