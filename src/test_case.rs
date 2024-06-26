use crate::test_events::{TestCaseBegin, TestCaseEnd, TestEvent};
use crate::{config::Config, test_context::TestCtx};
use bharat_cafe as bc;
use calamine::DataType;
use colored::Colorize;
use indicatif::ProgressBar;
use regex::Regex;
use reqwest::{Method, StatusCode, Url};
use serde_json::Value;
use std::default;
use std::{sync::mpsc::Sender, time::Duration};

// Possible test case results.
#[derive(Debug, Clone)]
pub enum TestResult {
    NotYetTested,
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct TestCase {
    pub id: u32,                        // test case identifier (typically a number)
    pub name: String,                   // human readable name for the test case.
    pub given: String,                  // test case description for the given condition (Given)
    pub when: String,                   // test case description for the then condition  (When)
    pub then: String,                   // test case description. for resulting condition. (Then)
    pub url: String,                    // URL of the request
    pub method: Method,                 // http method for the request.
    pub headers: Vec<(String, String)>, // http headers for the request, if any.
    //payload: Value,                 // payload (json) to be sent with the request.
    pub payload: String,     // payload to be sent with the request.
    pub sleep_duration: u64, // sleep duration after the request is sent.
    //pub expected_status: i32,             // expected http status code.
    pub is_authorizer: bool, // indicates if this is an authorization endpoint.
    pub is_authorized: bool, // indicates if this requires authorization.
    pub pre_test_script: Option<String>, // script to be executed before the test case.
    pub post_test_script: Option<String>, // script to be executed after the test case.

    pub errors: Vec<(String, String)>, // List of errors found while reading excel data.

    // fields that will be filled after test case is executed..
    //exec_duration: std::time::Duration,
    result: TestResult,
}

impl TestCase {
    // Initializes a test case object with a row of data from excel sheet.
    //pub fn new(row: &[&dyn calamine::DataType], config: &Config) -> Self {
    pub fn new(row: &[calamine::Data], config: &Config) -> Self {
        let mut errors = Vec::new();

        // Retrieve and evaluate the pre-test-script as the very first step,
        // as it may contain the code to setup JS runtime vars,
        // which may be consumed in other columns.
        let pre_test_script = match row[11].get_string() {
            //Some(s) => Some(s.to_owned()),
            Some(s) => Some(substitute_keywords(s)),
            None => None,
        };

        // Read the test case id.
        let id = match row[0].get_float() {
            Some(f) => f as u32,
            None => {
                errors.push(("id".to_owned(), "ID is not a number.".to_owned()));
                0
            }
        };

        // Test case name
        let name = match row[1].get_string() {
            //Some(s) => s.to_owned(),
            Some(s) => substitute_keywords(s),
            None => {
                errors.push(("name".to_owned(), "Invalid name field".to_owned()));
                "".to_string()
            }
        };

        // Test case's given condition
        let given = match row[2].get_string() {
            //Some(s) => s.to_owned(),
            Some(s) => substitute_keywords(s),
            None => {
                errors.push((
                    "given".to_owned(),
                    "Invalid data for 'given' field.".to_string(),
                ));
                "".to_string()
            }
        };

        // Testcase when condition
        let when = match row[3].get_string() {
            //Some(s) => s.to_owned(),
            Some(s) => substitute_keywords(s),
            None => {
                errors.push((
                    "when".to_owned(),
                    "Invalid data for 'when' field.".to_string(),
                ));
                "".to_string()
            }
        };

        // Test case's then result
        let then = match row[4].get_string() {
            //Some(s) => s.to_owned(),
            Some(s) => substitute_keywords(s),
            None => {
                errors.push((
                    "then".to_string(),
                    "Invalid data for 'then' field.".to_string(),
                ));
                "".to_string()
            }
        };

        // Test case URL
        let url = match row[5].get_string() {
            Some(s) => {
                let s = substitute_keywords(s);
                let full_url = format!(
                    "{}{}",
                    <std::option::Option<std::string::String> as Clone>::clone(&config.base_url)
                        .unwrap_or_default(),
                    s
                );
                match Url::parse(&full_url) {
                    Ok(_) => full_url,
                    Err(_) => {
                        errors.push(("url".to_string(), "Invalid URL format.".to_string()));
                        "".to_string()
                    }
                }
            }
            None => {
                errors.push(("url".to_string(), "No data for 'url' field.".to_string()));
                "".to_string()
            }
        };

        // Test case HTTP method
        let method = match row[6].get_string() {
            Some(s) => match s.parse::<reqwest::Method>() {
                Ok(m) => m,
                Err(_) => {
                    errors.push(("method".to_string(), "Invalid HTTP method.".to_string()));
                    Method::GET
                }
            },
            None => {
                errors.push((
                    "method".to_string(),
                    "No data for 'method' field.".to_string(),
                ));
                Method::GET
            }
        };

        // http headers if any for the request.
        let headers = match row[7].get_string() {
            Some(s) => s
                .split(',')
                .filter_map(|header| {
                    let parts: Vec<&str> = header.split(':').collect();
                    if parts.len() == 2 {
                        Some((parts[0].trim().to_owned(), parts[1].trim().to_owned()))
                    } else {
                        None
                    }
                })
                .collect(),
            None => Vec::new(),
        };

        // INput payload for the request, if the method is post, put or patch.
        let payload = match row[8].get_string() {
            Some(s) => {
                let substituted_s = substitute_keywords(s);
                match serde_json::from_str::<serde_json::Value>(&substituted_s) {
                    Ok(_) => substituted_s,
                    Err(_) => {
                        errors.push(("payload".to_string(), "Invalid JSON payload.".to_string()));
                        "".to_string()
                    }
                }
            }
            None => "".to_owned(),
        };

        /*
        let expected_status = match row[9].get_float() {
            Some(i) => match StatusCode::from_u16(i as u16) {
                Ok(s) => s.as_u16() as i32,
                Err(_) => {
                    errors.push((
                        "expected_status".to_string(),
                        "Invalid HTTP status code.".to_string(),
                    ));
                    0
                }
            },
            None => 0,
        };
        */

        let sleep_duration = match row[9].get_float() {
            Some(f) => f as u64,
            None => 0,
        };

        let (is_authorizer, is_authorized) = match row[10].get_string() {
            Some(s) => match s.to_lowercase().as_str() {
                "authorizer" => (true, false),
                "authorized" => (false, true),
                _ => (false, false),
            },
            None => (false, false),
        };

        let post_test_script = match row[12].get_string() {
            //Some(s) => Some(s.to_owned()),
            Some(s) => Some(substitute_keywords(s)),
            None => None,
        };

        let tc = TestCase {
            id,
            name,
            given,
            when,
            then,
            url,
            method,
            headers,
            payload,
            //expected_status,
            sleep_duration,
            is_authorized,
            is_authorizer,
            errors,
            pre_test_script,
            post_test_script,
            result: TestResult::NotYetTested,
            //exec_duration: Duration::from_secs(0),
        };
        //println!("Testcase data: {:?}", tc);
        tc
    }

    // Executes the test case, by using the provided http client  and an optional JWT token.
    // Returns an optional JWT token (if it was an authorization endpoint).
    pub fn run(
        &mut self,
        ts_ctx: &mut TestCtx,
        config: &Config,
        tx: &Sender<TestEvent>,
    ) -> TestResult {
        // Fire an event indicating that the test case execution has started.
        self.fire_start_evt(tx);

        println!("Running the test case: {}", self.name);

        // Verify if the test case has errors, if so return without executing.
        if self.errors.len() > 0 {
            println!(
                "Skipping test case: {} due to errors: {:?}",
                self.name, self.errors
            );
            return TestResult::Skipped;
        }

        // Execute pre_test_script if it exists
        if let Some(pre_test_script) = &self.pre_test_script {
            match ts_ctx.runtime.eval(pre_test_script) {
                Ok(_) => (),
                Err(e) => eprintln!("Error executing pre_test_script: {}", e),
            }
        }

        // Retrieve global variables and substitute placeholders in test case parameters
        self.name = self.substitute_placeholders(&self.name, ts_ctx);
        self.url = self.substitute_placeholders(&self.url, ts_ctx);
        self.payload = self.substitute_placeholders(&self.payload, ts_ctx);
        // TODO: for other columns..

        // if the test case is authorized, then add the jwt token to the headers.
        if self.is_authorized {
            if let Some(token) = ts_ctx.jwt_token.as_ref() {
                self.headers
                    .push(("Authorization".to_owned(), format!("Bearer {}", token)));
            }
        }

        // Frame the request based on Method type, add headers.
        let mut request = ts_ctx.client.request(self.method.clone(), &self.url);
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        // Only set the request body for HTTP methods that can have a request body.
        let payload: Value = serde_json::from_str(&self.payload).unwrap_or(serde_json::json!({}));
        match self.method {
            reqwest::Method::POST | reqwest::Method::PUT | reqwest::Method::PATCH => {
                request = request.json(&payload);
            }
            _ => {}
        }

        // Create a new progress bar
        let pb = ProgressBar::new_spinner();

        // Display a message to the user
        pb.set_message(format!("Fetching {}...", self.url));
        pb.enable_steady_tick(Duration::from_millis(100));

        // Fire the request using blocking call.
        ts_ctx.exec(request, self.is_authorizer, &config);

        // Stop progress animation
        pb.disable_steady_tick();

        // Execute the post test script and verify the result.
        let result = ts_ctx.verify_result(self.post_test_script.as_deref());

        // store the test result as an enum.
        let test_result = match result {
            true => TestResult::Passed,
            false => TestResult::Failed,
        };
        self.result = test_result;

        // Fire test case end evt.
        self.fire_end_evt(tx, ts_ctx);
        self.print_result(ts_ctx, config.verbose);

        // Sleep for the amount of time specified in the test case.
        println!("Sleeping for {} ms", self.sleep_duration);
        std::thread::sleep(Duration::from_millis(self.sleep_duration));

        self.result.clone()
    }

    fn fire_start_evt(&self, tx: &Sender<TestEvent>) {
        tx.send(TestEvent::EvtTestCaseBegin(self.get_start_evt_data()))
            .unwrap();
    }

    fn fire_end_evt(&self, tx: &Sender<TestEvent>, ts_ctx: &mut TestCtx) {
        tx.send(TestEvent::EvtTestCaseEnd(self.get_end_evt_data(ts_ctx)))
            .unwrap();
    }

    fn get_start_evt_data(&self) -> TestCaseBegin {
        TestCaseBegin {
            timestamp: std::time::Instant::now(),
            iteration_id: "1".to_string(),
            testcase_id: self.id,
            testcase_name: self.name.clone(),
            given: self.given.clone(),
            when: self.when.clone(),
            then: self.then.clone(),
            url: self.url.clone(),
            method: self.method.to_string(),
            headers: self.headers.clone(),
            payload: self.payload.clone(),
            pre_test_script: self.pre_test_script.clone(),
            post_test_script: self.post_test_script.clone(),
            is_authorizer: self.is_authorizer,
            is_authorized: self.is_authorized,
        }
    }

    fn get_end_evt_data(&self, ts_ctx: &mut TestCtx) -> TestCaseEnd {
        TestCaseEnd {
            timestamp: std::time::Instant::now(),
            iteration_id: "1".to_string(),
            testcase_id: self.id,
            exec_duration: Duration::from_secs(0),
            //TODO: Fix these below fields, to return properly filled values.
            status: self.get_exec_status(ts_ctx),
            response: self.get_exec_response(ts_ctx),
            response_json: self.get_exec_response_json(ts_ctx),
        }
    }

    fn get_exec_status(&self, ts_ctx: &mut TestCtx) -> i64 {
        ts_ctx
            .runtime
            .eval("SAT.response.status")
            .unwrap_or(default::Default::default())
            .as_i64()
            .unwrap_or(default::Default::default())
    }

    fn get_exec_response(&self, ts_ctx: &mut TestCtx) -> String {
        ts_ctx
            .runtime
            .eval("SAT.response.body")
            .unwrap_or(default::Default::default())
            .as_str()
            .unwrap_or(default::Default::default())
            .to_string()
    }

    fn get_exec_response_json(&self, ts_ctx: &mut TestCtx) -> Option<serde_json::Value> {
        match serde_json::from_str::<serde_json::Value>(&self.get_exec_response(ts_ctx)) {
            Ok(json) => Some(json),
            Err(_) => None,
        }
    }

    pub fn print_result(&self, ts_ctx: &mut TestCtx, verbose: bool) {
        println!("{:<15}: {}", "Test Case ID", self.id);
        println!("{:<15}: {}", "Test Case", self.name);
        println!("{:<15}: {}", "Given", self.given);
        println!("{:<15}: {}", "When", self.when);
        println!("{:<15}: {}", "Then", self.then);
        println!("{:<15}: {}", "Expected", ts_ctx.get_test_name());
        println!("{:<15}: {}", "Actual", ts_ctx.get_http_status());

        // print the below, if only verbose flag is enabled.
        if verbose {
            self.print_request_info();
            ts_ctx.print_response_info();
        }

        // finally print the pass / fail / skip status with symbols.
        match self.result {
            TestResult::Passed => println!("{:<15}: {}", "Result", "✅ PASSED".green()),
            TestResult::Failed => println!("{:<15}: {}", "Result", "❌ FAILED".red()),
            TestResult::Skipped => println!("{:<15}: {}", "Result", "⚠️ SKIPPED".yellow()),
            _ => (),
        }
    }

    pub fn print_request_info(&self) {
        println!("Request Info: ");
        println!("\tMethod: {:?}", self.method);
        println!("\tURL: {}", self.url);
        if !self.headers.is_empty() {
            println!("\tHeaders: ");
            for (key, value) in &self.headers {
                let value = value.replace("\n", "");
                println!("\t\t{}: {}", key, value);
            }
        }
        match self.method {
            reqwest::Method::POST | reqwest::Method::PUT | reqwest::Method::PATCH => {
                match serde_json::from_str::<serde_json::Value>(&self.payload) {
                    Ok(json) => {
                        let pretty_json = serde_json::to_string_pretty(&json).unwrap();
                        let indented_json = pretty_json.replace("\n", "\n\t\t");
                        println!("\tPayload: {}", indented_json);
                    }
                    Err(_) => println!("\tPayload: {}", &self.payload),
                }
            }
            _ => {}
        }
    }

    fn substitute_placeholders(&self, original: &str, ts_ctx: &mut TestCtx) -> String {
        let mut result = original.to_string();
        let re = Regex::new(r"\{\{(.*?)\}\}").unwrap();
        for cap in re.captures_iter(original) {
            let var_name = &cap[1];
            match ts_ctx.runtime.eval(&format!("SAT.globals.{}", var_name)) {
                Ok(value) => {
                    if let Some(value_str) = value.as_str() {
                        result = result.replace(&format!("{{{{{}}}}}", var_name), &value_str);
                    }
                }
                Err(_) => (),
            }
        }
        result
    }
}

fn substitute_keywords(input: &str) -> String {
    let mut output = input.to_string();
    if output.contains("$RandomName") {
        output = output.replace("$RandomName", &bc::random_name());
    }
    if output.contains("$RandomPhone") {
        output = output.replace("$RandomPhone", &bc::random_phone());
    }
    if output.contains("$RandomAddress") {
        output = output.replace("$RandomAddress", &bc::random_address());
    }
    if output.contains("$RandomCompany") {
        output = output.replace("$RandomCompany", &bc::generate_company_name());
    }
    if output.contains("$RandomEmail()") {
        output = output.replace("$RandomEmail()", &bc::random_email(None));
    }
    let re = Regex::new(r#"\$RandomEmail\("(.+?)"\)"#).unwrap();
    if let Some(captures) = re.captures(&output) {
        if let Some(domain) = captures.get(1) {
            let placeholder = format!("$RandomEmail(\"{}\")", domain.as_str());
            let replacement = bc::random_email(Some(domain.as_str()));
            output = output.replace(&placeholder, &replacement);
        }
    }
    output
}
