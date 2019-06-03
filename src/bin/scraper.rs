use lettre::{SmtpClient, Transport, ClientSecurity, ClientTlsParameters, smtp};
use lettre_email::Email;
use native_tls::TlsConnector;
use lettre::smtp::authentication::{Mechanism, Credentials};
use std::error::Error;
use log::{info, error, LevelFilter};
use std::env;
use std::io::Write;
use chrono::{NaiveDate, Utc};
use fda_calendar_scraper::fda_scraper;
use tempfile::NamedTempFile;
use fda_calendar_scraper::currency;
use askama::Template;
use fda_calendar_scraper::fda_scraper::{ScrapedCatalysts, ScrapeError};
use std::time::SystemTime;
use currency::USD;
use time::Duration;

fn main() {
    let overall_start_time = SystemTime::now();

    env_logger::builder()
        .filter_level(LevelFilter::Info) //defaults to error otherwise
//        .filter_module("lettre", LevelFilter::Debug)
//        .filter_module("lettre_email", LevelFilter::Debug)
        .init();

    let price_limit = USD::new(&env::var("PRICE_LIMIT").unwrap()[..]).unwrap();

    let date_limit = (Utc::today() + Duration::days(7)).naive_utc();

    let scrape_results = do_scraping("https://www.biopharmcatalyst.com/calendars/fda-calendar", &price_limit, &date_limit);

    match scrape_results {
        Ok(scrape_result) => send_email(&scrape_result),
        Err(err) => error!("Scraping Failed {}. Cause: {:?}", err, err.source())
    }

    if let Ok(overall_duration) = overall_start_time.elapsed() {
        info!("Overall took {} millis", overall_duration.as_millis());
    }

    //TODO probably rename this to fda_calendar_scraper.rs
}

//TODO shouldn't this be in the lib?
fn do_scraping(address_to_scrape: &str, price_limit: &currency::USD, date_limit: &NaiveDate) -> Result<ScrapedCatalysts, ScrapeError> {
    let download_start_time = SystemTime::now();
    let website_body = reqwest::get(address_to_scrape).unwrap().text().unwrap();

    //Duration can return errors if the system clock has been adjusted to prior to app start, etc
    //Since that's unlikely use if let to just log if the parsing went as expected
    // (we don't care for unexpected system clock events)
    if let Ok(download_duration) = download_start_time.elapsed() {
        info!("Download took {} millis", download_duration.as_millis());
    }

    let write_to_disk_start_time = SystemTime::now();
    let mut downloaded_fda_events: NamedTempFile = tempfile::NamedTempFile::new().unwrap();
    downloaded_fda_events.write_all(website_body.as_bytes()).unwrap();

    if let Ok(write_duration) = write_to_disk_start_time.elapsed() {
        info!("Write took {} millis", write_duration.as_millis());
    }

    let parsing_start_time = SystemTime::now();
    let parsing_result = fda_scraper::parse_rows_with_predicates(downloaded_fda_events.path(), &|x| x<= price_limit, &|x| x< date_limit);
    if let Ok(parsing_duration) = parsing_start_time.elapsed() {
        info!("Parsing took {} millis", parsing_duration.as_millis());
    }
    parsing_result
}

fn send_email(catalysts :&ScrapedCatalysts) {
    let email_start_time = SystemTime::now();

    let to_address = &env::var("TO_ADDRESS").unwrap()[..];
    let smtp_server = "smtp.gmail.com";
    //Can't move these out otherwise someone else looking at env::var wouldn't see them
    let smtp_username = &env::var("GMAIL_USERNAME").unwrap()[..];
    let smtp_password = &env::var("GMAIL_PASSWORD").unwrap()[..];

    let subject_date = Utc::now().format("%b %d %Y").to_string();

    let email = Email::builder()
        .to(to_address)
        .from(smtp_username)
        .subject(format!("Catalyst Update {}", subject_date))
        .html(catalysts.render().unwrap())
        .build()
        .unwrap();

    let security = ClientSecurity::Wrapper(ClientTlsParameters::new(smtp_server.to_string(), TlsConnector::new().unwrap()));
    let mut mailer = SmtpClient::new((smtp_server, smtp::SUBMISSIONS_PORT), security).unwrap()
        .authentication_mechanism(Mechanism::Login).credentials(Credentials::new(smtp_username.to_string(), smtp_password.to_string())).transport();

    // Send the email
    match mailer.send(email.into()) {
        Ok(_) => info!("Email Sent"),
        Err(err) => error!("failed to send email alert: {}. Cause: {:?}", err, err.source())
    }
    if let Ok(email_send_duration) = email_start_time.elapsed() {
        info!("Sending email took {} millis", email_send_duration.as_millis());
    }
}