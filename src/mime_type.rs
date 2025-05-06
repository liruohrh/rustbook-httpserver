use std::collections::HashMap;
use std::sync::LazyLock;

static CONTENT_TYPE_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    // HTML 相关
    map.insert("html", "text/html");
    map.insert("htm", "text/html");

    // CSS
    map.insert("css", "text/css");

    // JavaScript
    map.insert("js", "application/javascript");
    map.insert("mjs", "application/javascript");

    // 图片类型
    map.insert("jpg", "image/jpeg");
    map.insert("jpeg", "image/jpeg");
    map.insert("png", "image/png");
    map.insert("gif", "image/gif");
    map.insert("bmp", "image/bmp");
    map.insert("svg", "image/svg+xml");
    map.insert("webp", "image/webp");

    // 字体类型
    map.insert("ttf", "font/ttf");
    map.insert("otf", "font/otf");
    map.insert("woff", "font/woff");
    map.insert("woff2", "font/woff2");

    // 视频类型
    map.insert("mp4", "video/mp4");
    map.insert("webm", "video/webm");
    map.insert("ogg", "video/ogg");

    // 音频类型
    map.insert("mp3", "audio/mpeg");
    map.insert("wav", "audio/wav");

    // 其他常见类型
    map.insert("json", "application/json");
    map.insert("xml", "application/xml");
    map.insert("pdf", "application/pdf");
    map.insert("zip", "application/zip");
    map.insert("gz", "application/gzip");
    map.insert("txt", "text/plain");
    map
});

pub fn get_content_type(file_path: &str) -> &str {
    if let Some(extension) = file_path.split('.').last() {
        if let Some(content_type) = CONTENT_TYPE_MAP.get(extension) {
            return content_type;
        }
    }
    "application/octet-stream"
}
