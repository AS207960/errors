#![feature(decl_macro)]

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate serde;

use chrono::prelude::*;
use rocket::response::Responder;

struct FormatJSON {}

struct FormatXML {}

#[derive(Serialize)]
struct TemplateContext {
    info: ErrorInfo,
    identifier: String,
}

#[derive(Serialize)]
struct ErrorInfo {
    status_code: u16,
    status_reason: &'static str,
    original_uri: String,
    namespace: String,
    ingress_name: String,
    service_name: String,
    service_port: u16,
    request_id: String,
    method: &'static str,
}

#[derive(Serialize)]
struct ErrorInfoJSON<'a> {
    #[serde(rename = "c")]
    code: u16,
    #[serde(rename = "o")]
    original_uri: &'a str,
    #[serde(rename = "n")]
    namespace: &'a str,
    #[serde(rename = "i")]
    ingress_name: &'a str,
    #[serde(rename = "s")]
    service_name: &'a str,
    #[serde(rename = "p")]
    service_port: u16,
    #[serde(rename = "d")]
    request_id: &'a str,
    #[serde(rename = "t")]
    timestamp: DateTime<Utc>,
    #[serde(rename = "m")]
    method: &'a str,
}

impl ErrorInfo {
    fn to_identifier(&self) -> String {
        let info_json = ErrorInfoJSON {
            code: self.status_code,
            original_uri: &self.original_uri,
            namespace: &self.namespace,
            ingress_name: &self.ingress_name,
            service_name: &self.service_name,
            service_port: self.service_port,
            request_id: &self.request_id,
            timestamp: Utc::now(),
            method: self.method,
        };
        let json = match serde_cbor::to_vec(&info_json) {
            Ok(d) => d,
            Err(_) => return String::new()
        };
        base64::encode_config(&json, base64::URL_SAFE_NO_PAD)
    }
}

impl<'a, 'r> rocket::request::FromRequest<'a, 'r> for FormatJSON {
    type Error = ();

    fn from_request(req: &'a rocket::Request<'r>) -> rocket::request::Outcome<Self, Self::Error> {
        if let Some(accept) = req.headers().get_one("X-Format").and_then(
            |v| v.parse::<rocket::http::Accept>().ok()
        ) {
            let preferred_mt = accept.preferred().media_type();

            if preferred_mt == &rocket::http::MediaType::JSON {
                rocket::request::Outcome::Success(FormatJSON {})
            } else {
                rocket::request::Outcome::Forward(())
            }
        } else {
            rocket::Outcome::Forward(())
        }
    }
}

impl<'a, 'r> rocket::request::FromRequest<'a, 'r> for FormatXML {
    type Error = ();

    fn from_request(req: &'a rocket::Request<'r>) -> rocket::request::Outcome<Self, Self::Error> {
        if let Some(accept) = req.headers().get_one("X-Format").and_then(
            |v| v.parse::<rocket::http::Accept>().ok()
        ) {
            let preferred_mt = accept.preferred().media_type();

            if preferred_mt == &rocket::http::MediaType::XML {
                rocket::request::Outcome::Success(FormatXML {})
            } else {
                rocket::request::Outcome::Forward(())
            }
        } else {
            rocket::Outcome::Forward(())
        }
    }
}

impl<'a, 'r> rocket::request::FromRequest<'a, 'r> for ErrorInfo {
    type Error = &'static str;

    fn from_request(req: &'a rocket::Request<'r>) -> rocket::request::Outcome<Self, Self::Error> {
        let headers = req.headers();
        let method = req.method().as_str();

        let code = match headers.get_one("X-Code") {
            Some(h) => match h.parse::<u16>() {
                Ok(c) => match rocket::http::Status::from_code(c) {
                    Some(c) => c,
                    None => return rocket::request::Outcome::Failure((rocket::http::Status::InternalServerError, "Invalid X-Code header"))
                },
                Err(_) => return rocket::request::Outcome::Failure((rocket::http::Status::InternalServerError, "Invalid X-Code header"))
            },
            None => return rocket::request::Outcome::Forward(())
        };

        let original_uri = match headers.get_one("X-Original-URI") {
            Some(h) => h.to_string(),
            None => return rocket::request::Outcome::Forward(())
        };

        let namespace = headers.get_one("X-Namespace").map(str::to_string).unwrap_or_default();
        let ingress_name = headers.get_one("X-Ingress-Name").map(str::to_string).unwrap_or_default();
        let service_name = headers.get_one("X-Service-Name").map(str::to_string).unwrap_or_default();
        let service_port = headers.get_one("X-Service-Port").and_then(|v| str::parse::<u16>(v).ok()).unwrap_or_default();

        let request_id = match headers.get_one("X-Request-ID") {
            Some(h) => h.to_string(),
            None => return rocket::request::Outcome::Failure((rocket::http::Status::InternalServerError, "X-Request-ID header not present"))
        };

        rocket::request::Outcome::Success(ErrorInfo {
            status_code: code.code,
            status_reason: code.reason,
            original_uri,
            namespace,
            ingress_name,
            service_name,
            service_port,
            request_id,
            method,
        })
    }
}

#[derive(Serialize)]
struct ProblemJSON {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    problem_type: Option<String>,
    title: &'static str,
    status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instance: Option<String>,
}

impl<'a> rocket::response::Responder<'a> for ProblemJSON {
    fn respond_to(self, _: &rocket::request::Request) -> rocket::response::Result<'a> {
        let res_body = match serde_json::to_vec(&self) {
            Ok(b) => b,
            Err(_) => return rocket::response::Result::Err(rocket::http::Status::InternalServerError)
        };
        rocket::response::Response::build()
            .raw_header("Content-Type", "application/problem+json")
            .sized_body(std::io::Cursor::new(res_body))
            .raw_status(self.status, self.title)
            .ok()
    }
}

struct ProblemXML {
    problem_type: Option<String>,
    title: &'static str,
    status: u16,
    detail: Option<String>,
    instance: Option<String>,
}

impl<'a> rocket::response::Responder<'a> for ProblemXML {
    fn respond_to(self, _: &rocket::request::Request) -> rocket::response::Result<'a> {
        let mut body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"><problem xmlns=\"urn:ietf:rfc:7807\">\
<title>{}</title><status>{}</status>", self.title, self.status);
        if let Some(problem_type) = self.problem_type {
            body.push_str(&format!("<type>{}</type>", problem_type));
        }
        if let Some(detail) = self.detail {
            body.push_str(&format!("<detail>{}</detail>", detail));
        }
        if let Some(instance) = self.instance {
            body.push_str(&format!("<instance>{}</instance>", instance));
        }
        body.push_str("</problem>");

        rocket::response::Response::build()
            .raw_header("Content-Type", "application/problem+xml")
            .sized_body(std::io::Cursor::new(body))
            .raw_status(self.status, self.title)
            .ok()
    }
}

fn handler_json(error_info: ErrorInfo) -> ProblemJSON {
    ProblemJSON {
        problem_type: None,
        title: error_info.status_reason,
        status: error_info.status_code,
        detail: None,
        instance: Some(error_info.to_identifier()),
    }
}

#[get("/", rank = 1)]
fn handler_json_get(_json: FormatJSON, error_info: ErrorInfo) -> ProblemJSON {
    handler_json(error_info)
}

#[put("/", rank = 1)]
fn handler_json_put(_json: FormatJSON, error_info: ErrorInfo) -> ProblemJSON {
    handler_json(error_info)
}

#[post("/", rank = 1)]
fn handler_json_post(_json: FormatJSON, error_info: ErrorInfo) -> ProblemJSON {
    handler_json(error_info)
}

#[delete("/", rank = 1)]
fn handler_json_delete(_json: FormatJSON, error_info: ErrorInfo) -> ProblemJSON {
    handler_json(error_info)
}

#[head("/", rank = 1)]
fn handler_json_head(_json: FormatJSON, error_info: ErrorInfo) -> ProblemJSON {
    handler_json(error_info)
}

#[options("/", rank = 1)]
fn handler_json_options(_json: FormatJSON, error_info: ErrorInfo) -> ProblemJSON {
    handler_json(error_info)
}

#[patch("/", rank = 1)]
fn handler_json_patch(_json: FormatJSON, error_info: ErrorInfo) -> ProblemJSON {
    handler_json(error_info)
}


fn handler_xml(error_info: ErrorInfo) -> ProblemXML {
    ProblemXML {
        problem_type: None,
        title: error_info.status_reason,
        status: error_info.status_code,
        detail: None,
        instance: Some(error_info.to_identifier()),
    }
}

#[get("/", rank = 2)]
fn handler_xml_get(_xml: FormatXML, error_info: ErrorInfo) -> ProblemXML {
    handler_xml(error_info)
}

#[put("/", rank = 2)]
fn handler_xml_put(_xml: FormatXML, error_info: ErrorInfo) -> ProblemXML {
    handler_xml(error_info)
}

#[post("/", rank = 2)]
fn handler_xml_post(_xml: FormatXML, error_info: ErrorInfo) -> ProblemXML {
    handler_xml(error_info)
}

#[delete("/", rank = 2)]
fn handler_xml_delete(_xml: FormatXML, error_info: ErrorInfo) -> ProblemXML {
    handler_xml(error_info)
}

#[head("/", rank = 2)]
fn handler_xml_head(_xml: FormatXML, error_info: ErrorInfo) -> ProblemXML {
    handler_xml(error_info)
}

#[options("/", rank = 2)]
fn handler_xml_options(_xml: FormatXML, error_info: ErrorInfo) -> ProblemXML {
    handler_xml(error_info)
}

#[patch("/", rank = 2)]
fn handler_xml_patch(_xml: FormatXML, error_info: ErrorInfo) -> ProblemXML {
    handler_xml(error_info)
}

fn handler_html(error_info: ErrorInfo) -> rocket::response::status::Custom<rocket_contrib::templates::Template> {
    let status = rocket::http::Status {
        code: error_info.status_code,
        reason: error_info.status_reason,
    };
    let context = TemplateContext {
        identifier: error_info.to_identifier(),
        info: error_info,
    };
    rocket::response::status::Custom(status, rocket_contrib::templates::Template::render("base", &context))
}

#[get("/", rank = 3)]
fn handler_html_get(error_info: ErrorInfo) -> rocket::response::status::Custom<rocket_contrib::templates::Template> {
    handler_html(error_info)
}

#[put("/", rank = 3)]
fn handler_html_put(error_info: ErrorInfo) -> rocket::response::status::Custom<rocket_contrib::templates::Template> {
    handler_html(error_info)
}

#[post("/", rank = 3)]
fn handler_html_post(error_info: ErrorInfo) -> rocket::response::status::Custom<rocket_contrib::templates::Template> {
    handler_html(error_info)
}

#[delete("/", rank = 3)]
fn handler_html_delete(error_info: ErrorInfo) -> rocket::response::status::Custom<rocket_contrib::templates::Template> {
    handler_html(error_info)
}

#[head("/", rank = 3)]
fn handler_html_head(error_info: ErrorInfo) -> rocket::response::status::Custom<rocket_contrib::templates::Template> {
    handler_html(error_info)
}

#[options("/", rank = 3)]
fn handler_html_options(error_info: ErrorInfo) -> rocket::response::status::Custom<rocket_contrib::templates::Template> {
    handler_html(error_info)
}

#[patch("/", rank = 3)]
fn handler_html_patch(error_info: ErrorInfo) -> rocket::response::status::Custom<rocket_contrib::templates::Template> {
    handler_html(error_info)
}

#[get("/healthz")]
fn health() -> rocket::http::Status {
    rocket::http::Status::Ok
}

#[derive(Serialize)]
struct Blank {}

fn handle_404<'r>(req: &'r rocket::Request) -> rocket::response::Result<'r> {
    let accept = req.accept();
    
    let mut res = if let Some(mt) = accept {
        if mt == &rocket::http::Accept::JSON {
            let res = ProblemJSON {
                problem_type: None,
                title: "Not found",
                status: 404,
                detail: Some("We don't know where to direct your request, we're probably not responsible for this domain.".to_string()),
                instance: None,
            };
            res.respond_to(req)
        } else if mt == &rocket::http::Accept::XML {
            let res = ProblemXML {
                problem_type: None,
                title: "Not found",
                status: 404,
                detail: Some("We don't know where to direct your request, we're probably not responsible for this domain.".to_string()),
                instance: None,
            };
            res.respond_to(req)
        } else {
            let res = rocket::response::status::NotFound(rocket_contrib::templates::Template::render("unknown", &Blank {}));
            res.respond_to(req)
        }
    } else {
        let res = rocket::response::status::NotFound(rocket_contrib::templates::Template::render("unknown", &Blank {}));
        res.respond_to(req)
    }?;

    res.set_status(rocket::http::Status::Ok);
    Ok(res)
}

fn main() {
    rocket::ignite()
        .attach(rocket_contrib::templates::Template::fairing())
        .register(vec![rocket::Catcher::new(404, handle_404)])
        .mount("/", routes![
            handler_json_get, handler_json_put, handler_json_post, handler_json_delete,
            handler_json_head, handler_json_options, handler_json_patch,

            handler_xml_get, handler_xml_put, handler_xml_post, handler_xml_delete,
            handler_xml_head, handler_xml_options, handler_xml_patch,

            handler_html_get, handler_html_put, handler_html_post, handler_html_delete,
            handler_html_head, handler_html_options, handler_html_patch,

            health])
        .launch();
}
