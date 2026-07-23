pub(super) fn diagnose_json(error: &serde_json::Error, context: &str) {
    let category = match error.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    };
    eprintln!(
        "pandar network plugin JSON failed: context={context} category={category} line={} column={}",
        error.line(),
        error.column()
    );
}
