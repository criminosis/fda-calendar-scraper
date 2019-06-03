use lettre::{SmtpClient, Transport, ClientSecurity, ClientTlsParameters, smtp};
use lettre_email::Email;
use native_tls::TlsConnector;
use lettre::smtp::authentication::{Mechanism, Credentials};
use std::error::Error;
use log::{info, error, LevelFilter};
use std::env;
use chrono::Utc;
use fda_calendar_scraper::{currency, fda_scraper, fda_scraper::ScrapedCatalysts};
use askama::Template;
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

    let scrape_results = fda_scraper::do_scraping("https://www.biopharmcatalyst.com/calendars/fda-calendar", &price_limit, &date_limit);

    match scrape_results {
        Ok(scrape_result) => send_email(&scrape_result),
        Err(err) => error!("Scraping Failed {}. Cause: {:?}", err, err.source())
    }

    if let Ok(overall_duration) = overall_start_time.elapsed() {
        info!("Overall took {} millis", overall_duration.as_millis());
    }
}

fn send_email(catalysts: &ScrapedCatalysts) {
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