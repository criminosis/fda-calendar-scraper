use lettre::{SmtpClient, Transport, ClientSecurity, ClientTlsParameters, smtp};
use lettre_email::Email;
use native_tls::TlsConnector;
use lettre::smtp::authentication::{Mechanism, Credentials};
use std::error::Error;
use log::{info, warn, error, LevelFilter};
use std::env;
use std::fs::File;
use std::io::Write;
use chrono::NaiveDate;
use fda_calendar_scraper::fda_scraper;
use fda_calendar_scraper::html_render;
use tempfile::NamedTempFile;
use fda_calendar_scraper::currency;
use std::path::Path;
use askama::Template;

fn main() {
    env_logger::builder()
        .filter_level(LevelFilter::Info) //defaults to error otherwise
//        .filter_module("lettre", LevelFilter::Debug)
//        .filter_module("lettre_email", LevelFilter::Debug)
        .init();
    info!("test logging");
    warn!("Test warn");
    error!("Test error");
    let x = fda_scraper::parse_all_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_sample.html")).unwrap();
    println!("{}", x.render().unwrap());

    //TODO we need an address to target to download
    //Download to some tmp file
    //Parse tmp file of web page
    //Submit email
    //Delete tmp file

    //TODO log how long it took to do a run. Separate out processing time and emailing time and then do total

    //TODO delete the _download.html file that was previously used for testing

    //TODO probably rename this to fda_calendar_scraper.rs
}

//"https://www.biopharmcatalyst.com/calendars/fda-calendar"
fn do_scraping(address_to_scrape: &str, price_limit: &currency::USD, date_limit: &NaiveDate) {
    //TODO handle web scraping? Maybe more testing here, passing in url, returning a SendEmail?
    //TODO handle all the unwraps
    let website_body = reqwest::get(address_to_scrape).unwrap().text().unwrap();

    let mut downloaded_fda_events: NamedTempFile = tempfile::NamedTempFile::new().unwrap();
    downloaded_fda_events.write_all(website_body.as_bytes()).unwrap();
    let parsing_result = fda_scraper::parse_rows_with_predicates(downloaded_fda_events.path(), &|x| x<= price_limit, &|x| x< date_limit);
}


//TODO this will take in some string that pretty prints to the email body, etc
//Alternatively just make our post scrape data type implement SendableEmail
//http://siciarz.net/24-days-rust-lettre/
fn send_email() {
    let to_address = &env::var("TO_ADDRESS").unwrap()[..];
    let smtp_server = "smtp.gmail.com";
    //Can't move these out otherwise someone else looking at env::var wouldn't see them
    let smtp_username = &env::var("GMAIL_USERNAME").unwrap()[..];
    let smtp_password = &env::var("GMAIL_PASSWORD").unwrap()[..];

    let email = Email::builder()
        .to(to_address)
        .from(smtp_username)
        .subject("Hi, Hello world")
        .text("Hello world.")
        .build()
        .unwrap();

    let security = ClientSecurity::Wrapper(ClientTlsParameters::new(smtp_server.to_string(), TlsConnector::new().unwrap()));
    let mut mailer = SmtpClient::new((smtp_server, smtp::SUBMISSIONS_PORT), security).unwrap()
        .authentication_mechanism(Mechanism::Login).credentials(Credentials::new(smtp_username.to_string(), smtp_password.to_string())).transport();

    // Send the email
    let result = mailer.send(email.into());

    //assert!(result.is_ok());
    match result {
        Ok(_) => println!("email sent"),
        Err(err) => println!("failed to send email alert: {}. Cause: {:?}", err, err.source())
    }
}