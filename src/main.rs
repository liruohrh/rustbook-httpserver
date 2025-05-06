use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, BufRead, BufReader, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{self, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use std::time::Duration;

type HttpHandler = fn(ctx: &mut Context);
type MiddlewareFunc = fn(chain: &mut MiddlewareChain, ctx: &mut Context);

#[derive(Debug)]
struct RequestMapping {
    method: HttpMethod,
    path: String,
    handler: HttpHandler,
}

struct Context {
    request: HttpRequest,
    response: Option<HttpResponse>,
}
impl Context {
    fn set_response(&mut self, response: HttpResponse) {
        self.response = Some(response);
    }
}

#[derive(Debug)]
struct Middleware {
    method: Option<HttpMethod>,
    path: String,
    order: usize,
    handler: MiddlewareFunc,
}
impl Middleware {
    fn new(handler: MiddlewareFunc) -> Self {
        Middleware {
            method: None,
            path: "/**".to_string(),
            order: 0,
            handler,
        }
    }
    fn method(mut self, method: HttpMethod) -> Self {
        self.method = Some(method);
        self
    }
    fn path(mut self, path: String) -> Self {
        self.path = path;
        self
    }
    fn order(mut self, order: usize) -> Self {
        self.order = order;
        self
    }
}

struct MiddlewareChain<'a> {
    handler: HttpHandler,
    middlewares: Vec<&'a Middleware>,
    abort_index: i8,
    index: i8,
}

impl<'a> MiddlewareChain<'a> {
    fn new(handler: HttpHandler, middlewares: Vec<&'a Middleware>) -> Self {
        MiddlewareChain {
            handler,
            middlewares,
            abort_index: -1,
            index: 0,
        }
    }
    fn is_abort(&self) -> bool {
        self.abort_index != -1
    }
    fn abort(&mut self) {
        self.abort_index = self.index;
    }
    fn next(&mut self, ctx: &mut Context) {
        if self.index < self.middlewares.len() as i8 && !self.is_abort() {
            let i = self.index as usize;
            let middleware = self.middlewares.get(i);
            if let Some(md) = middleware {
                self.index += 1;
                (md.handler)(self, ctx);
                return;
            }
        }
        (self.handler)(ctx);
    }
}

#[derive(Debug, PartialEq, Clone)]
enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    HEAD,
    OPTIONS,
    TRACE,
}
impl HttpMethod {
    fn name_of(name: String) -> Option<HttpMethod> {
        match name.as_str() {
            "GET" => Some(HttpMethod::GET),
            "POST" => Some(HttpMethod::POST),
            "PUT" => Some(HttpMethod::PUT),
            "DELETE" => Some(HttpMethod::DELETE),
            "HEAD" => Some(HttpMethod::HEAD),
            "OPTIONS" => Some(HttpMethod::OPTIONS),
            "TRACE" => Some(HttpMethod::TRACE),
            _ => None,
        }
    }
}

struct HttpServer {
    address: String,
    middlewares: Vec<Middleware>,
    handlers: Vec<RequestMapping>,
    view_root: Option<String>,
}
impl HttpServer {
    fn new(address: String) -> HttpServer {
        HttpServer {
            address,
            middlewares: Vec::new(),
            handlers: Vec::new(),
            view_root: None,
        }
    }
    fn add_middleware(&mut self, middleware: Middleware) {
        self.middlewares.push(middleware)
    }
    fn add_handler(&mut self, method: HttpMethod, path: String, handler: HttpHandler) {
        self.handlers.push(RequestMapping {
            method,
            handler,
            path,
        });
    }

    fn run(&self) {
        let listener = TcpListener::bind(&self.address).unwrap();
        for stream in listener.incoming() {
            let mut _stream = stream.unwrap();
            let response = match parse_http_request(&_stream) {
                Ok(request) => self.dispatch_request(request),
                Err(()) => {
                    _stream.shutdown(Shutdown::Both).unwrap();
                    None
                }
            };
            if let Some(resp) = response {
                self.handler_response(&mut _stream, resp);
            }
        }
    }
    fn dispatch_request(&self, request: HttpRequest) -> Option<HttpResponse> {
        let handler = self
            .handlers
            .iter()
            .find(|mapping| mapping.method == request.method && mapping.path == request.path);
        match handler {
            None => Some(HttpResponse::new(404)),
            Some(mapping) => {
                println!("[{}]: match {:?} {}", format_now(), mapping.method, mapping.path);
                let matched_middlewares = self
                    .middlewares
                    .iter()
                    .filter(|m| {
                        (m.method.clone().is_none_or(|m| m == request.method))
                            && (m.path == "/**"
                                || (m.path.ends_with("/**")
                                    && m.path.replace("/**", "") == request.path)
                                || m.path == request.path)
                    })
                    .collect::<Vec<&Middleware>>();
                let mut chain = MiddlewareChain::new((*mapping).handler, matched_middlewares);
                let mut ctx = Context {
                    request,
                    response: None,
                };
                chain.next(&mut ctx);
                ctx.response
            }
        }
    }

    fn handler_response(&self, stream: &mut TcpStream, mut response: HttpResponse) {
        if let Some(body) = response.body.as_ref() {
            self.write_response_line_header(stream,  &response);
            stream.write(body.as_bytes()).unwrap();
        } else if let Some(view) = response.view.as_ref() {
            let view_path = match self.view_root.as_ref() {
                Some(root) => {
                    Path::new(root).join(view)
                }
                None => PathBuf::from(view),
            };
            println!("[{}]: look for view: {:?}", format_now(), view_path);
            match File::open(&view_path) {
                Ok(ref mut file) => {
                    self.write_response_line_header(stream,  &response);
                    io::copy(file, stream).unwrap();
                }
                Err(e) => {
                    println!("Error opening file: {} {:?}", e, view_path);
                    response.status_code = 404;
                    if let Some(headers) = response.headers.as_mut() {
                        headers.remove("Content-Type");
                    }
                    self.write_response_line_header(stream,  &response);
                }
            }
        }
    }

    fn write_response_line_header(&self, stream: &mut TcpStream, response:  &HttpResponse) {
        let message = match response.status_code {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Unknown Error",
        }
            .to_string();
        let response_line: String = format!("HTTP/1.1 {} {}\r\n", response.status_code, message);

        stream.write(response_line.as_bytes()).unwrap();
        if let Some(ref headers) = response.headers {
            for (key, value) in headers.iter() {
                let header_line = format!("{}: {}\r\n", key, value);
                stream.write(header_line.as_bytes()).unwrap();
            }
        }
        stream.write(b"\r\n").unwrap();
    }
}
fn format_now()->String{
    format_datetime(SystemTime::now(), offset8())
}
fn offset8() -> Option<Duration> {
    Some(Duration::from_secs(8 * 60 * 60))
}

fn main() {
    let mut http_server = HttpServer::new("127.0.0.1:8080".into());
    http_server.view_root = Some("./template".into());
    http_server.add_middleware(Middleware::new(|chain, ctx| {
        println!(
            "[{}]: [{}] {:?} {}",
            format_now(),
            ctx.request.remote_addr, ctx.request.method, ctx.request.path
        );
        chain.next(ctx)
    }));
    http_server.add_handler(HttpMethod::GET, "/".into(), |ctx| {
        ctx.set_response(HttpResponse::view("index.html".into()));
    });
    http_server.run();
}

#[derive(Debug)]
struct HttpRequest {
    remote_addr: String,
    method: HttpMethod,
    path: String,
    version: String,
    headers: HashMap<String, String>,
    body: Option<String>,
}

#[derive(Debug)]
struct HttpResponse {
    status_code: u16,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    view: Option<String>,
}
impl HttpResponse {
    fn view(view_name: String) -> HttpResponse {
        HttpResponse {
            status_code: 200,
            headers: Some(HashMap::from([(
                "Content-Type".to_string(),
                "text/html".to_string(),
            )])),
            body: None,
            view: Some(view_name.into()),
        }
    }
    fn json(json: String) -> HttpResponse {
        HttpResponse {
            status_code: 200,
            headers: Some(HashMap::from([(
                "Content-Type".to_string(),
                "application/json".to_string(),
            )])),
            body: Some(json),
            view: None,
        }
    }
    fn new(status_code: u16) -> HttpResponse {
        HttpResponse {
            status_code,
            headers: None,
            body: None,
            view: None,
        }
    }
    fn status_code(mut self, status_code: u16) -> Self {
        self.status_code = status_code;
        self
    }
    fn headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = Some(headers);
        self
    }
    fn add_header(mut self, key: String, value: String) -> Self {
        if self.headers.is_none() {
            self.headers = Some(HashMap::new());
        }
        self.headers.as_mut().unwrap().insert(key, value);
        self
    }
    fn body(mut self, body: String) -> Self {
        self.body = Some(body);
        self
    }
}

// 解析 HTTP 请求
fn parse_http_request(stream: &TcpStream) -> Result<HttpRequest, ()> {
    let lines = BufReader::new(stream)
        .lines()
        .map(|line| line.unwrap())
        .take_while(|line| !line.is_empty())
        .collect::<Vec<String>>();

    if lines.is_empty() {
        return Err(());
    }
    // 解析请求行
    let request_line = lines[0].split_whitespace().collect::<Vec<&str>>();
    if request_line.len() != 3 {
        return Err(());
    }
    let method = request_line[0].to_string();
    let path = request_line[1].to_string();
    let version = request_line[2].to_string();

    // 解析请求头
    let mut headers = std::collections::HashMap::new();
    let mut i = 1;
    while i < lines.len() && !lines[i].is_empty() {
        let parts: Vec<&str> = lines[i].splitn(2, ": ").collect();
        if parts.len() == 2 {
            headers.insert(parts[0].to_string(), parts[1].to_string());
        }
        i += 1;
    }

    // 解析请求体
    let body = if i + 1 < lines.len() {
        Some(lines[i + 1..].join("\r\n"))
    } else {
        None
    };

    let remote_addr = stream.peer_addr();
    if let Err(_) = remote_addr {
        return Err(());
    }
    Ok(HttpRequest {
        remote_addr: remote_addr.unwrap().to_string(),
        method: HttpMethod::name_of(method.to_uppercase()).unwrap(),
        path,
        version,
        headers,
        body,
    })
}

fn pong(mut stream: TcpStream) {
    println!("request from  {}", stream.peer_addr().unwrap());
    BufReader::new(&stream)
        .lines()
        .map(|line| line.unwrap())
        .take_while(|line| !line.is_empty())
        .for_each(|line| println!("{line}"));
    println!("{}", "=".repeat(100));

    let now = SystemTime::now();
    let response = format!("HTTP/1.1 200 OK\r\n\r\npong at {}", format_now());
    stream.write(response.as_bytes()).unwrap();
}

fn format_datetime(system_time: SystemTime, offset: Option<Duration>) -> String {
    let duration = system_time.duration_since(UNIX_EPOCH).unwrap();
    let mut seconds = duration.as_secs();
    if let Some(offset) = offset {
        seconds += offset.as_secs();
    }
    let epoch_year = 1970;
    let days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let mut year = epoch_year;
    while {
        let is_leap = is_leap_year(year);
        let days_in_year = if is_leap { 366 } else { 365 };
        seconds >= days_in_year * 86400
    } {
        let is_leap = is_leap_year(year);
        let days_in_year = if is_leap { 366 } else { 365 };
        seconds -= days_in_year * 86400;
        year += 1;
    }

    let is_leap = is_leap_year(year);
    let mut month = 0;
    while {
        let days = days_in_month[month] + if month == 1 && is_leap { 1 } else { 0 };
        seconds >= days * 86400
    } {
        let days = days_in_month[month] + if month == 1 && is_leap { 1 } else { 0 };
        seconds -= days * 86400;
        month += 1;
    }

    let day = (seconds / 86400) + 1;
    seconds %= 86400;
    let hour = seconds / 3600;
    seconds %= 3600;
    let minute = seconds / 60;
    let second = seconds % 60;

    return format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year,
        month + 1,
        day,
        hour,
        minute,
        second
    );
}

// 判断是否为闰年
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
