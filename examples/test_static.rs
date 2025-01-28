/*let validate_files = Box::new(|path: &str| {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > 5 * 1024 * 1024 {
        return Err(HttpError::Message("File too large".to_string()));
    }

    let extension = std::path::Path::new(path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or(HttpError::Message("Invalid extension".to_string()))?;

    match extension.to_lowercase().as_str() {
        "html" | "css" | "js" | "png" | "jpg" => Ok(()),
        _ => Err(HttpError::Message("File type not allowed".to_string())),
    }
});

let result = http_server.serve_static(
    "/static",
    "examples/static_test/",
    ".",
    &Some(validate_files),
);
match result {
    Ok(_) => println!("Serving static files"),
    Err(e) => {
        println!("Error serving static files: {}", e);
    }
}*/
