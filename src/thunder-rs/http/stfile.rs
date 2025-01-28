use std::collections::HashSet;
use std::path::PathBuf;

use hyper::{body::Bytes, Response, StatusCode};
use serde_json::Value;

use super::{
    error::HttpError,
    routes::ContentHeader,
    server::{empty, HttpServer},
};

impl HttpServer {
    /// Serves static files from a directory with proper content types and security checks
    /// Handles HTML files and their referenced CSS/JS files automatically
    /// Example:
    /// ```
    /// server.serve_static(
    ///     "/static",           // URL path
    ///     "./web/static",      // Directory to serve
    ///     "./web",            // Root directory for security
    ///     None,               // Optional validation function
    /// )?;
    /// ```
    /// Security features:
    /// 1. Path traversal prevention via canonicalization
    /// 2. Root directory validation
    /// 3. Optional custom validation function
    ///
    /// Automatic handling:
    /// 1. HTML files: Scans for CSS/JS references
    /// 2. CSS files: Both <link> tags and relative/absolute paths
    /// 3. JS files: Both <script> tags and ES6 modules
    /// 4. Other files: Served with appropriate content types
    /// 5. Favicon: Returns 204 No Content
    ///
    /// Validation function example:
    /// ```
    /// let validate_files = Box::new(|path: &str| {
    ///     // Check file size (5MB limit)
    ///     let metadata = std::fs::metadata(path)?;
    ///     if metadata.len() > 5 * 1024 * 1024 {
    ///         return Err(HttpError::Message("File too large".to_string()));
    ///     }
    ///
    ///     // Check file extension
    ///     let extension = std::path::Path::new(path)
    ///         .extension()
    ///         .and_then(std::ffi::OsStr::to_str)
    ///         .ok_or(HttpError::Message("Invalid extension".to_string()))?;
    ///
    ///     match extension.to_lowercase().as_str() {
    ///         "html" | "css" | "js" | "png" | "jpg" => Ok(()),
    ///         _ => Err(HttpError::Message("File type not allowed".to_string()))
    ///     }
    /// });
    /// ```
    ///
    /// Cache headers are set to private with 1-hour max age
    pub fn serve_static(
        &mut self,
        url: &str,
        path: &str,
        root: &str,
        validate: &Option<Box<dyn Fn(&str) -> Result<(), HttpError> + 'static>>,
    ) -> Result<(), HttpError> {
        let path = std::fs::canonicalize(path)?;
        let path_str = path
            .to_str()
            .ok_or(HttpError::Message("Invalid path".to_string()))?
            .to_string();

        let root = std::fs::canonicalize(root)?;
        let root_str = root
            .to_str()
            .ok_or(HttpError::Message("Invalid root".to_string()))?
            .to_string();

        if !path_str.starts_with(&root_str) {
            return Err(HttpError::Message("Access denied".to_string()));
        }

        if path.is_dir() {
            let mut html_handler = None;
            let mut other_files = Vec::new();
            let mut css_references = HashSet::new();
            let mut js_references = HashSet::new();

            for entry in std::fs::read_dir(path.clone())? {
                let entry = entry?;
                let target_path = entry.path();

                let file_name = target_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or(HttpError::Message("Invalid file name".to_string()))?;

                let extension = target_path
                    .extension()
                    .and_then(std::ffi::OsStr::to_str)
                    .ok_or(HttpError::Message("Invalid file extension".to_string()))?;

                if extension == "html" {
                    if let Ok(content) = std::fs::read_to_string(&target_path) {
                        let css_re = regex::Regex::new(r#"href=["']([^"']+\.css)["']"#).unwrap();
                        for cap in css_re.captures_iter(&content) {
                            if let Some(css_path) = cap.get(1) {
                                let css_path = css_path.as_str();
                                let html_dir = target_path.parent().unwrap();

                                let absolute_file_path = if css_path.starts_with("/") {
                                    PathBuf::from(root.clone()).join(&css_path[1..])
                                } else {
                                    html_dir.join(css_path)
                                };

                                match absolute_file_path.canonicalize() {
                                    Ok(canonical_path) => {
                                        if let Some(css_filename) = canonical_path.file_name() {
                                            if let Some(css_filename_str) = css_filename.to_str() {
                                                let browser_path = format!("/{}", css_filename_str);
                                                css_references
                                                    .insert((browser_path, canonical_path));
                                            }
                                        }
                                    }
                                    Err(e) => println!("Failed to canonicalize CSS path: {}", e),
                                }
                            }
                        }

                        let js_re = regex::Regex::new(r#"src=["']([^"']+\.js)["']"#).unwrap();
                        for cap in js_re.captures_iter(&content) {
                            if let Some(js_path) = cap.get(1) {
                                let js_path = js_path.as_str();
                                let html_dir = target_path.parent().unwrap();

                                let absolute_file_path = if js_path.starts_with("/") {
                                    PathBuf::from(root.clone()).join(&js_path[1..])
                                } else {
                                    html_dir.join(js_path)
                                };

                                match absolute_file_path.canonicalize() {
                                    Ok(canonical_path) => {
                                        if let Some(js_filename) = canonical_path.file_name() {
                                            if let Some(js_filename_str) = js_filename.to_str() {
                                                let browser_path = format!("/{}", js_filename_str);
                                                js_references
                                                    .insert((browser_path, canonical_path));
                                            }
                                        }
                                    }
                                    Err(e) => println!("Failed to canonicalize JS path: {}", e),
                                }
                            }
                        }

                        let module_re =
                            regex::Regex::new(r#"type=["']module["']\s+src=["']([^"']+\.js)["']"#)
                                .unwrap();
                        for cap in module_re.captures_iter(&content) {
                            if let Some(js_path) = cap.get(1) {
                                let js_path = js_path.as_str();
                                let html_dir = target_path.parent().unwrap();

                                let absolute_file_path = if js_path.starts_with("/") {
                                    PathBuf::from(root.clone()).join(&js_path[1..])
                                } else {
                                    html_dir.join(js_path)
                                };

                                match absolute_file_path.canonicalize() {
                                    Ok(canonical_path) => {
                                        if let Some(js_filename) = canonical_path.file_name() {
                                            if let Some(js_filename_str) = js_filename.to_str() {
                                                let browser_path = format!("/{}", js_filename_str);
                                                js_references
                                                    .insert((browser_path, canonical_path));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        println!("Failed to canonicalize JS module path: {}", e)
                                    }
                                }
                            }
                        }
                    }
                }

                let content_type = match Self::get_content_type(extension) {
                    Ok(content_type) => content_type,
                    Err(e) => return Err(HttpError::Message(e.to_string())),
                };

                if let Some(validate) = &validate {
                    validate(
                        target_path
                            .to_str()
                            .ok_or(HttpError::Message("Invalid target path".to_string()))?,
                    )?;
                }

                let target_path_clone = target_path.clone();
                let handler = move |_: Bytes,
                                    _: Value,
                                    _headers: hyper::HeaderMap|
                      -> Result<String, HttpError> {
                    match std::fs::read(&target_path_clone) {
                        Ok(file) => match String::from_utf8(file) {
                            Ok(content) => {
                                let mut response = Response::new(content);
                                response.headers_mut().insert(
                                    hyper::header::CACHE_CONTROL,
                                    hyper::header::HeaderValue::from_static(
                                        "private, max-age=3600",
                                    ),
                                );
                                response.headers_mut().insert(
                                    hyper::header::CONTENT_TYPE,
                                    hyper::header::HeaderValue::from_static(
                                        content_type.content_type(),
                                    ),
                                );
                                Ok(response.into_body())
                            }
                            Err(e) => Err(HttpError::Utf8(e)),
                        },
                        Err(e) => Err(HttpError::IoError(e)),
                    }
                };

                let file_url = format!("{}/{}", url.trim_end_matches('/'), file_name);

                if extension == "html" {
                    let direct_url = format!("/{}", file_name);
                    self.get(&direct_url, handler.clone(), Some(content_type));
                    html_handler = Some((handler.clone(), content_type));
                } else if extension == "css" || extension == "js" {
                    continue;
                } else {
                    other_files.push((file_url, handler, content_type));
                }
            }

            let favicon_handler = move |_: Bytes,
                                        _: Value,
                                        _headers: hyper::HeaderMap|
                  -> Result<String, HttpError> {
                let mut response = Response::new(empty());
                *response.status_mut() = StatusCode::NO_CONTENT;
                response.headers_mut().insert(
                    hyper::header::CONTENT_TYPE,
                    hyper::header::HeaderValue::from_static(
                        ContentHeader::TextPlain.content_type(),
                    ),
                );
                Ok(String::new())
            };

            self.get(
                "/favicon.ico",
                favicon_handler,
                Some(ContentHeader::TextPlain),
            );

            for (browser_path, canonical_path) in &css_references {
                let target_path_clone = canonical_path.clone();
                let handler = move |_: Bytes,
                                    _: Value,
                                    _headers: hyper::HeaderMap|
                      -> Result<String, HttpError> {
                    match std::fs::read(&target_path_clone) {
                        Ok(file) => match String::from_utf8(file) {
                            Ok(content) => {
                                let mut response = Response::new(content);
                                response.headers_mut().insert(
                                    hyper::header::CACHE_CONTROL,
                                    hyper::header::HeaderValue::from_static(
                                        "private, max-age=3600",
                                    ),
                                );
                                response.headers_mut().insert(
                                    hyper::header::CONTENT_TYPE,
                                    hyper::header::HeaderValue::from_static(
                                        ContentHeader::TextCss.content_type(),
                                    ),
                                );
                                Ok(response.into_body())
                            }
                            Err(e) => Err(HttpError::Utf8(e)),
                        },
                        Err(e) => Err(HttpError::IoError(e)),
                    }
                };

                self.get(browser_path, handler, Some(ContentHeader::TextCss));
            }

            for (browser_path, canonical_path) in &js_references {
                let target_path_clone = canonical_path.clone();
                let handler = move |_: Bytes,
                                    _: Value,
                                    _headers: hyper::HeaderMap|
                      -> Result<String, HttpError> {
                    match std::fs::read(&target_path_clone) {
                        Ok(file) => match String::from_utf8(file) {
                            Ok(content) => {
                                let mut response = Response::new(content);
                                response.headers_mut().insert(
                                    hyper::header::CACHE_CONTROL,
                                    hyper::header::HeaderValue::from_static(
                                        "private, max-age=3600",
                                    ),
                                );
                                response.headers_mut().insert(
                                    hyper::header::CONTENT_TYPE,
                                    hyper::header::HeaderValue::from_static(
                                        ContentHeader::ApplicationJavascript.content_type(),
                                    ),
                                );
                                Ok(response.into_body())
                            }
                            Err(e) => Err(HttpError::Utf8(e)),
                        },
                        Err(e) => Err(HttpError::IoError(e)),
                    }
                };

                self.get(
                    browser_path,
                    handler,
                    Some(ContentHeader::ApplicationJavascript),
                );
            }

            if let Some((handler, content_type)) = html_handler {
                self.get(url, handler, Some(content_type));
            }

            for (file_url, handler, content_type) in other_files {
                self.get(&file_url, handler, Some(content_type));
            }
        } else {
            return Err(HttpError::Message("Path is not a directory".to_string()));
        }
        Ok(())
    }

    fn get_content_type(extension: &str) -> Result<ContentHeader, &'static str> {
        let extension = extension.to_lowercase();
        match extension.as_str() {
            "txt" => Ok(ContentHeader::TextPlain),
            "json" => Ok(ContentHeader::ApplicationJson),
            "xml" => Ok(ContentHeader::ApplicationXml),
            "bin" => Ok(ContentHeader::ApplicationOctetStream),
            "html" => Ok(ContentHeader::TextHtml),
            "css" => Ok(ContentHeader::TextCss),
            "js" => Ok(ContentHeader::ApplicationJavascript),
            _ => Err("Unsupported file extension"),
        }
    }
}
