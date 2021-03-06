use std::fs;
use scraper::{Html, Selector};
use chrono::NaiveDate;
use std::collections::BTreeMap;
use log::{info, error};
use std::fmt;
use std::error;
use std::io;
use scraper::ElementRef;
use super::currency;
use std::path::Path;
use askama::Template;
use std::time::SystemTime;
use tempfile::NamedTempFile;
use std::io::Write;

#[derive(Debug, Eq, PartialEq)]
pub struct ParsedRow {
    pub price: currency::USD,
    pub url: String,
    pub symbol: String,
    pub catalyst_date: NaiveDate,
    pub drug_name: String,
    pub drug_indication: String,
    pub catalyst_note: String,
    pub phase: String,
}

//We have multiple errors possible, so enumerate them here so we have a common wrapping to match & deconstruct on
#[derive(Debug)]
pub enum ScrapeError {
    InvalidSelector(String),
    ExpectedFieldNotFound(Selector),
    DateParseFailure(chrono::format::ParseError),
    FileReadError(io::Error),
    CurrencyParseError(currency::USDParseError),
}

impl From<currency::USDParseError> for ScrapeError {
    fn from(e: currency::USDParseError) -> Self {
        ScrapeError::CurrencyParseError(e)
    }
}

impl From<chrono::format::ParseError> for ScrapeError {
    fn from(e: chrono::format::ParseError) -> Self {
        ScrapeError::DateParseFailure(e)
    }
}

impl fmt::Display for ScrapeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ScrapeError::ExpectedFieldNotFound(ref e) => write!(f, "Didn't match css selector {:?}", e),
            ScrapeError::InvalidSelector(ref e) => write!(f, "Malformed CSS Selector {:?}", e),
            // This is a wrapper, so defer to the underlying types' implementation of `fmt`.
            //ScrapeError::InvalidSelector(ref e) => e.fmt(f),

            //chrono::ParseError implements Display and Debug both of which have a fmt function, as such just calling e.fmt is ambiguous
            ScrapeError::DateParseFailure(ref e) => std::fmt::Display::fmt(&e, f),
            ScrapeError::FileReadError(ref e) => std::fmt::Display::fmt(&e, f),
            ScrapeError::CurrencyParseError(ref e) => std::fmt::Display::fmt(&e, f),
        }
    }
}

impl error::Error for ScrapeError {
    //cause is depreciated, use source instead
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            ScrapeError::ExpectedFieldNotFound(_) => None,
            ScrapeError::InvalidSelector(_) => None,
            ScrapeError::DateParseFailure(ref e) => Some(e),
            ScrapeError::FileReadError(ref e) => Some(e),
            ScrapeError::CurrencyParseError(ref e) => Some(e),
        }
    }
}

//New type to make sure only PhaseLabels are used as the key type in ScrapedCatalysts. Zero cost abstraction
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
struct PhaseLabel(String);

impl fmt::Display for PhaseLabel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Eq, PartialEq, Template)]
// Template will generate the code...
#[template(path = "scraped_catalysts_email_body.html")] // using the template in this path, relative
pub struct ScrapedCatalysts {
    catalysts: BTreeMap<(PhaseLabel, NaiveDate), Vec<ParsedRow>>
}

fn build_selector_for(css_selector: &str) -> Selector {
    Selector::parse(css_selector).expect(format!("Malformed CSS Selector {:?}", css_selector).as_str())
}

//for a lifetime called 'a is borrowed from the lifetime of the given element ref,
// the returned &str's lifetime needs to be no longer than the given ElementRef
fn select_first_element_from<'a>(an_element_ref: &ElementRef<'a>, selector: &Selector) -> Result<ElementRef<'a>, ScrapeError> {
    //Ok or else so it only invokes clone when it needs to. Ok_or would clone each time even though it would be discarded
    an_element_ref.select(selector).nth(0).ok_or_else(||ScrapeError::ExpectedFieldNotFound(selector.clone()))
}

fn select_first_text_from<'a>(an_element_ref: &ElementRef<'a>, selector: &Selector) -> Result<&'a str, ScrapeError> {
    retrieve_text_from(&select_first_element_from(an_element_ref, selector)?, selector)
}

fn retrieve_text_from<'a>(an_element_ref: &ElementRef<'a>, underlying_selector: &Selector) -> Result<&'a str, ScrapeError> {
    an_element_ref.text().next().map(|x| x.trim()).ok_or_else(||ScrapeError::ExpectedFieldNotFound(underlying_selector.clone()))
}

fn retrieve_attr_from<'a>(an_element_ref: &ElementRef<'a>, attr: &str, underlying_selector: &Selector) -> Result<&'a str, ScrapeError> {
    an_element_ref.value().attr(attr).ok_or_else(||ScrapeError::ExpectedFieldNotFound(underlying_selector.clone()))
}


impl ScrapedCatalysts {
    //Should price_limit and retrieval_limit be some kind of predicates instead?
    fn new(document: &Html, scrape_predicate: (&Fn(&currency::USD) -> bool, &Fn(&NaiveDate) -> bool)) -> Result<ScrapedCatalysts, ScrapeError> {

        let (price_limit, date_limit) = scrape_predicate;

        let event_table_row_selector = build_selector_for("tr.js-tr.js-drug");
        let price = build_selector_for("div[class=price]");
        let symbol_and_url = build_selector_for("td a[href]");
        let catalyst_date = build_selector_for("time[class=catalyst-date]");
        let drug_name = build_selector_for("strong[class=drug]");
        let drug_indication = build_selector_for("div[class=indication]");
        let catalyst_note = build_selector_for("div[class=catalyst-note]");
        let phase = build_selector_for("td.js-td--stage[data-value]");

        let mut catalysts = BTreeMap::new();
        for an_event_table_row in document.select(&event_table_row_selector) {

            let price = currency::USD::new(select_first_text_from(&an_event_table_row, &price)?)?;

            let catalyst_date = NaiveDate::parse_from_str(select_first_text_from(&an_event_table_row, &catalyst_date)?, "%m/%d/%Y")?;

            if !price_limit(&price) || !date_limit(&catalyst_date) {
                continue; //if this isn't within our date or price limits we should skip it
            }

            let url_symbol_ref = select_first_element_from(&an_event_table_row, &symbol_and_url)?;
            let url = retrieve_attr_from(&url_symbol_ref, "href", &symbol_and_url)?.to_owned();
            let symbol = retrieve_text_from(&url_symbol_ref, &symbol_and_url)?.to_owned();

            let drug_name = select_first_text_from(&an_event_table_row, &drug_name)?.to_owned();
            let drug_indication = select_first_text_from(&an_event_table_row, &drug_indication)?.to_owned();
            let catalyst_note = select_first_text_from(&an_event_table_row, &catalyst_note)?.to_owned();


            let phase_element = select_first_element_from(&an_event_table_row, &phase)?;
            let phase_grouping = PhaseLabel(retrieve_attr_from(&phase_element, "data-value", &phase)?.to_owned());
            let phase = retrieve_text_from(&phase_element, &phase)?.to_owned();

            let to_insert = ParsedRow { price, url, symbol, catalyst_date, drug_name, drug_indication, catalyst_note, phase };

            catalysts.entry((phase_grouping, catalyst_date)).or_insert(Vec::new()).push(to_insert);
        }

        Ok(ScrapedCatalysts { catalysts })
    }
}

pub fn do_scraping(address_to_scrape: &str, price_limit: currency::USD, date_limit: NaiveDate) -> Result<ScrapedCatalysts, ScrapeError> {
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
    let parsing_result = parse_rows(downloaded_fda_events.path(),
                                    ScrapePredicates::new().set_price_limit(price_limit).set_date_limit(date_limit));
    if let Ok(parsing_duration) = parsing_start_time.elapsed() {
        info!("Parsing took {} millis", parsing_duration.as_millis());
    }
    parsing_result
}

pub struct ScrapePredicates {
    price_limit: Option<currency::USD>,
    date_limit: Option<NaiveDate>
}

impl ScrapePredicates {
    pub fn new() -> ScrapePredicates {
        ScrapePredicates {price_limit: Option::None, date_limit: Option::None}
    }

    pub fn set_price_limit(mut self, price_limit: currency::USD) -> Self {
        self.price_limit = Option::Some(price_limit);
        self
    }

    pub fn set_date_limit(mut self, date_limit: NaiveDate) -> Self {
        self.date_limit = Option::Some(date_limit);
        self
    }

    fn test_price(&self, test_value: &currency::USD) -> bool {
        match &self.price_limit {
            Some(price_limit) => test_value <= price_limit,
            None => true //if no limit was set
        }
    }

    fn test_date(&self, test_value: &NaiveDate) -> bool {
        match &self.date_limit {
            Some(date_limit) => test_value < date_limit,
            None => true //if no limit was set
        }
    }
}

pub fn parse_rows(file_path: &Path, predicates: ScrapePredicates) -> Result<ScrapedCatalysts, ScrapeError> {
    let parsed = fs::read_to_string(file_path)
        .map_err(|x| ScrapeError::FileReadError(x))
        .map(|contents| Html::parse_document(&contents))
        .and_then(|document| ScrapedCatalysts::new(&document, (&|x| predicates.test_price(x), &|x| predicates.test_date(x))));
    match parsed {
        Ok(_) => info!("parsed = {:?}", parsed),
        Err(_) => error!("failed = {:?}", parsed),
    }
    parsed
}

//Child modules can access "private" module things, so for this test module to access the
//ParsedRow struct, that's private, we need to be a child module
#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use std::collections::btree_map::BTreeMap;
    use chrono::NaiveDate;

    #[test]
    #[should_panic(expected = "Failed to parse date")]
    fn parse_malformed_time() {
        match parse_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_malformed_date.html"), ScrapePredicates::new()) {
            Err(ScrapeError::DateParseFailure(_)) => panic!("Failed to parse date"),
            x => panic!("Unexpected result {:?}", x)
        }
    }

    #[test]
    #[should_panic(expected = "Failed to parse currency")]
    fn parse_malformed_price() {
        match parse_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_malformed_price.html"), ScrapePredicates::new()) {
            Err(ScrapeError::CurrencyParseError(_)) => panic!("Failed to parse currency"),
            x => panic!("Unexpected result {:?}", x)
        }
    }

    #[test]
    fn parse_single_row() {
        let expected_row = ParsedRow {
            price: currency::USD::new("$1.26").unwrap(),
            url: "https://www.biopharmcatalyst.com/company/BTX".to_string(),
            symbol: "BTX".to_string(),
            catalyst_date: NaiveDate::parse_from_str("2019-05-02", "%Y-%m-%d").unwrap(),
            drug_name: "OpRegen".to_string(),
            drug_indication: "Dry age-related macular degeneration (AMD)".to_string(),
            catalyst_note: "Phase 1/2 enrolment to be completed 2019. Updated data due  May 2, 2019,10:15am ET at ARVO.".to_string(),
            phase: "Phase 1/2".to_string(),
        };

        let mut catalysts = BTreeMap::new();
        catalysts.insert((PhaseLabel("phase1.5".to_string()), NaiveDate::parse_from_str("2019-05-02", "%Y-%m-%d").unwrap()), vec![expected_row]);

        assert_eq!(ScrapedCatalysts { catalysts },
                   parse_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_sample.html"), ScrapePredicates::new()).unwrap());
    }

    #[test]
    fn parse_multiple_rows() {
        let expected_row1 = ParsedRow {
            price: currency::USD::new("$1.26").unwrap(),
            url: "https://www.biopharmcatalyst.com/company/BTX".to_string(),
            symbol: "BTX".to_string(),
            catalyst_date: NaiveDate::parse_from_str("2019-05-02", "%Y-%m-%d").unwrap(),
            drug_name: "OpRegen".to_string(),
            drug_indication: "Dry age-related macular degeneration (AMD)".to_string(),
            catalyst_note: "Phase 1/2 enrolment to be completed 2019. Updated data due  May 2, 2019,10:15am ET at ARVO.".to_string(),
            phase: "Phase 1/2".to_string(),
        };

        let expected_row2 = ParsedRow {
            price: currency::USD::new("$173.16").unwrap(),
            url: "https://www.biopharmcatalyst.com/company/GWPH".to_string(),
            symbol: "GWPH".to_string(),
            catalyst_date: NaiveDate::parse_from_str("2019-05-03", "%Y-%m-%d").unwrap(),
            drug_name: "Epidiolex GWPCARE2".to_string(),
            drug_indication: "Dravet Syndrome".to_string(),
            catalyst_note: "Phase 3 data to be presented at AAN in late-breaker May 7, 2019. Abstract embargoed until May 3, 2019.".to_string(),
            phase: "Phase 3".to_string(),
        };

        let expected_row3 = ParsedRow {
            price: currency::USD::new("$6.00").unwrap(),
            url: "https://www.biopharmcatalyst.com/company/EYEN".to_string(),
            symbol: "EYEN".to_string(),
            catalyst_date: NaiveDate::parse_from_str("2019-05-03", "%Y-%m-%d").unwrap(),
            drug_name: "MicroStat".to_string(),
            drug_indication: "Mydriasis - pupil dilation".to_string(),
            catalyst_note: "Phase 3 trial met primary endpoint - January 31, 2019. Data from second trial also met primary endpoint - February 25, 2019. Detailed data due at (ASCRS) meeting May 3-7, 2019.".to_string(),
            phase: "Phase 3".to_string(),
        };

        let mut catalysts = BTreeMap::new();
        catalysts.insert((PhaseLabel("phase1.5".to_string()), NaiveDate::parse_from_str("2019-05-02", "%Y-%m-%d").unwrap()), vec![expected_row1]);
        catalysts.insert((PhaseLabel("phase3".to_string()), NaiveDate::parse_from_str("2019-05-03", "%Y-%m-%d").unwrap()), vec![expected_row2, expected_row3]);

        assert_eq!(ScrapedCatalysts { catalysts }, parse_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_multiple_rows.html"), ScrapePredicates::new()).unwrap());
    }

    #[test]
    fn parse_with_price_ceiling() {
        let expected_row1 = ParsedRow {
            price: currency::USD::new("$1.26").unwrap(),
            url: "https://www.biopharmcatalyst.com/company/BTX".to_string(),
            symbol: "BTX".to_string(),
            catalyst_date: NaiveDate::parse_from_str("2019-05-02", "%Y-%m-%d").unwrap(),
            drug_name: "OpRegen".to_string(),
            drug_indication: "Dry age-related macular degeneration (AMD)".to_string(),
            catalyst_note: "Phase 1/2 enrolment to be completed 2019. Updated data due  May 2, 2019,10:15am ET at ARVO.".to_string(),
            phase: "Phase 1/2".to_string(),
        };

        let expected_row2 = ParsedRow {
            price: currency::USD::new("$6.00").unwrap(),
            url: "https://www.biopharmcatalyst.com/company/EYEN".to_string(),
            symbol: "EYEN".to_string(),
            catalyst_date: NaiveDate::parse_from_str("2019-05-03", "%Y-%m-%d").unwrap(),
            drug_name: "MicroStat".to_string(),
            drug_indication: "Mydriasis - pupil dilation".to_string(),
            catalyst_note: "Phase 3 trial met primary endpoint - January 31, 2019. Data from second trial also met primary endpoint - February 25, 2019. Detailed data due at (ASCRS) meeting May 3-7, 2019.".to_string(),
            phase: "Phase 3".to_string(),
        };

        let mut catalysts = BTreeMap::new();
        catalysts.insert((PhaseLabel("phase1.5".to_string()), NaiveDate::parse_from_str("2019-05-02", "%Y-%m-%d").unwrap()), vec![expected_row1]);
        catalysts.insert((PhaseLabel("phase3".to_string()), NaiveDate::parse_from_str("2019-05-03", "%Y-%m-%d").unwrap()), vec![expected_row2]);

        assert_eq!(ScrapedCatalysts { catalysts },
                   parse_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_multiple_rows.html"),
                              ScrapePredicates::new().set_price_limit(currency::USD::new("$6").unwrap())).unwrap());
    }

    #[test]
    fn parse_with_date_ceiling() {
        let expected_row = ParsedRow {
            price: currency::USD::new("$1.26").unwrap(),
            url: "https://www.biopharmcatalyst.com/company/BTX".to_string(),
            symbol: "BTX".to_string(),
            catalyst_date: NaiveDate::parse_from_str("2019-05-02", "%Y-%m-%d").unwrap(),
            drug_name: "OpRegen".to_string(),
            drug_indication: "Dry age-related macular degeneration (AMD)".to_string(),
            catalyst_note: "Phase 1/2 enrolment to be completed 2019. Updated data due  May 2, 2019,10:15am ET at ARVO.".to_string(),
            phase: "Phase 1/2".to_string(),
        };

        let mut catalysts = BTreeMap::new();
        catalysts.insert((PhaseLabel("phase1.5".to_string()), NaiveDate::parse_from_str("2019-05-02", "%Y-%m-%d").unwrap()), vec![expected_row]);

        let date_limit = NaiveDate::parse_from_str("2019-05-03", "%Y-%m-%d").unwrap();
        assert_eq!(ScrapedCatalysts { catalysts },
                   parse_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_multiple_rows.html"),
                                              ScrapePredicates::new().set_date_limit(date_limit)).unwrap());
    }

    #[test]
    fn parse_with_price_and_date_ceiling() {
        let price_limit = currency::USD::new("$1").unwrap();
        let date_limit = NaiveDate::parse_from_str("2019-05-03", "%Y-%m-%d").unwrap();

        assert_eq!(ScrapedCatalysts { catalysts: BTreeMap::new()},
                   parse_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_multiple_rows.html"),
                                              ScrapePredicates::new().set_price_limit(price_limit).set_date_limit(date_limit)).unwrap());
    }
}