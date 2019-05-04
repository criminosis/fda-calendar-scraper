

//use fda_calendar_scraper::fda_scraper;
use lettre::{SmtpClient, Transport, ClientSecurity, ClientTlsParameters, smtp};
use lettre_email::Email;
use native_tls::TlsConnector;
use lettre::smtp::authentication::{Mechanism, Credentials};
use std::error::Error;
use log::{info, warn, error, LevelFilter};
use std::env;

fn main() {
    env_logger::builder()
        .filter_level(LevelFilter::Info) //defaults to error otherwise
//        .filter_module("lettre", LevelFilter::Debug)
//        .filter_module("lettre_email", LevelFilter::Debug)
        .init();
    info!("test logging");
    warn!("Test warn");
    error!("Test error");
    send_email()
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