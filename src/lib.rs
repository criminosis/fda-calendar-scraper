pub mod fda_scraper {
    use std::fs;
    use scraper::{Html, Selector};
    use chrono::NaiveDate;
    use std::collections::BTreeMap;
    use currency::Currency;
    use log::{info, error};
    use std::fmt;
    use std::error;
    use std::io;
    use scraper::ElementRef;

    #[derive(Debug, Eq, PartialEq)]
    struct ParsedRow {
        price: Currency,
        url: String,
        symbol: String,
        catalyst_date: NaiveDate,
        drug_name: String,
        drug_indication: String,
        catalyst_note: String,
        phase: String,
    }

    //We have multiple errors possible, so enumerate them here so we have a common wrapping to match & deconstruct on
    #[derive(Debug)]
    pub enum ScrapeError {
        InvalidSelector(String),
        ExpectedFieldNotFound(Selector),
        DateParseFailure(chrono::format::ParseError),
        FileReadError(io::Error),
        CurrencyParseError(currency::ParseCurrencyError),
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
        fn cause(&self) -> Option<&error::Error> {
            match *self {
                // The cause is the underlying implementation error type. Is implicitly
                // cast to the trait object `&error::Error`. This works because the
                // underlying type already implements the `Error` trait.
                ScrapeError::ExpectedFieldNotFound(_) => None,
                ScrapeError::InvalidSelector(_) => None,
                ScrapeError::DateParseFailure(ref e) => Some(e),
                ScrapeError::FileReadError(ref e) => Some(e),
                ScrapeError::CurrencyParseError(ref e) => Some(e),
            }
        }
    }

    //For each type of error we want to wrap we need to provide a From
//    impl From<parser::ParseError<selectors::parser::SelectorParseErrorKind>> for ScrapeError {
//        fn from(x: parser::ParseError<selectors::parser::SelectorParseErrorKind>) -> Self {
//            ScrapeError(x)
//        }
//    }

    /*
    Iterating bounds of dataset
     for ( ref key, ref value) in map.range((Included(("foo".to_string(), "c".to_string())), Excluded(("g".to_string(), "".to_string())))) {
    println!("{:?}: {}", key, value);
    }
    */

    #[derive(Debug, Eq, PartialEq)]
    pub struct ScrapedCatalysts {
        //TODO convert the string to PhaseLabel new type?
        catalysts: BTreeMap<(String, NaiveDate), Vec<ParsedRow>>
        //TODO we need to do some kind of filtering by the price?
        //  The into() conversation, while cool, may be a bad match here because we want to also include price in filtering
        //  We'd also want to apply a time filtering to say when to maybe early out the iterator (assuming it's date ordered)
        //TODO implement SendableEmail to turn this returned type into an email to send
        //  can we get the first entry, and then submap all the matching entries for a given phase, then tail map to the next batch?
    }

    fn build_selector_for(css_selector: &str) -> Result<Selector, ScrapeError> {
        Selector::parse(css_selector)
            .map_err(|x| ScrapeError::InvalidSelector(format!("Malformed CSS Selector {:?}", &x)))
    }

    //for a lifetime called 'a is borrowed from the lifetime of the given element ref,
    // the returned &str's lifetime needs to be no longer than the given ElementRef
    fn select_first_element_from<'a>(an_element_ref: &ElementRef<'a>, selector: &Selector) -> Option<ElementRef<'a>> {
        an_element_ref.select(selector).nth(0)
    }

    fn select_first_text_from<'a>(an_element_ref: &ElementRef<'a>, selector: &Selector) -> Result<&'a str, ScrapeError> {
        extraction_cleanup(select_first_element_from(an_element_ref, selector)
                               .and_then(|x| x.text().next()), selector)
    }

    fn extraction_cleanup<'a>(to_cleanup: Option<&'a str>, underlying_selector: &Selector) -> Result<&'a str, ScrapeError> {
        to_cleanup.map(|x| x.trim())
            //Ok or else so it only invokes clone when it needs to. Ok_or would clone each time even though it would be discarded
            .ok_or_else(|| ScrapeError::ExpectedFieldNotFound(underlying_selector.clone()))
    }

    fn retrieve_text_from<'a>(an_element_ref: &ElementRef<'a>, underlying_selector: &Selector) -> Result<String, ScrapeError> {
        extraction_cleanup(an_element_ref.text().next(), underlying_selector).map(|x| x.to_string())
    }

    fn retrieve_attr_from<'a>(an_element_ref: &ElementRef<'a>, attr: &str, underlying_selector: &Selector) -> Result<String, ScrapeError> {
        extraction_cleanup(an_element_ref.value().attr(attr), underlying_selector).map(|x| x.to_string())
    }

    fn select_first_string<'a>(an_element_ref: &ElementRef<'a>, selector: &Selector) -> Result<String, ScrapeError> {
        select_first_text_from(an_element_ref, selector).map(|x| x.to_string())
    }

    impl ScrapedCatalysts {
        //Should price_limit and retrieval_limit be some kind of predicates instead?
        pub fn new(document: &Html, scrape_predicate: (Currency, NaiveDate)) -> Result<ScrapedCatalysts, ScrapeError> {
            let event_table_row_selector = build_selector_for("tr.js-tr.js-drug")?;
            let price = build_selector_for("div[class=price]")?;
            let symbol_and_url = build_selector_for("td a[href]")?;
            let catalyst_date = build_selector_for("time[class=catalyst-date]")?;
            let drug_name = build_selector_for("strong[class=drug]")?;
            let drug_indication = build_selector_for("div[class=indication]")?;
            let catalyst_note = build_selector_for("div[class=catalyst-note]")?;
            let phase = build_selector_for("td.js-td--stage[data-value]")?;

            let mut catalysts = BTreeMap::new();
            for an_event_table_row in document.select(&event_table_row_selector) {

                let price = select_first_text_from(&an_event_table_row, &price)
                    .and_then(|x| Currency::from_str(&x)
                        .map_err(|x| ScrapeError::CurrencyParseError(x)))?;

                let url_symbol_ref = select_first_element_from(&an_event_table_row, &symbol_and_url)
                    .ok_or_else(||ScrapeError::ExpectedFieldNotFound(symbol_and_url.clone()))?;
                let url = retrieve_attr_from(&url_symbol_ref, "href", &symbol_and_url)?;
                let symbol = retrieve_text_from(&url_symbol_ref, &symbol_and_url)?;

                let catalyst_date = select_first_text_from(&an_event_table_row, &catalyst_date)
                    .and_then(|x| NaiveDate::parse_from_str(x, "%m/%d/%Y")
                        .map_err(|x| ScrapeError::DateParseFailure(x)))?;

                let drug_name = select_first_string(&an_event_table_row, &drug_name)?;
                let drug_indication = select_first_string(&an_event_table_row, &drug_indication)?;
                let catalyst_note = select_first_string(&an_event_table_row, &catalyst_note)?;


                let phase_element = select_first_element_from(&an_event_table_row, &phase)
                    .ok_or_else(|| ScrapeError::ExpectedFieldNotFound(phase.clone()))?;
                let phase_grouping = retrieve_attr_from(&phase_element, "data-value", &phase)?;
                let phase = retrieve_text_from(&phase_element, &phase)?;

                let to_insert = ParsedRow { price, url, symbol, catalyst_date, drug_name, drug_indication, catalyst_note, phase };

                catalysts.entry((phase_grouping, catalyst_date)).or_insert(Vec::new()).push(to_insert);
            }

            Ok(ScrapedCatalysts { catalysts })
        }
    }

    pub fn parse(file_path: &str) -> Result<ScrapedCatalysts, ScrapeError> {
        let parsed = fs::read_to_string(file_path)
            .map_err(|x| ScrapeError::FileReadError(x))
            .map(|contents| Html::parse_document(&contents))
            .and_then(|document| ScrapedCatalysts::new(&document, (Currency::from_str("$99.00").unwrap(), NaiveDate::parse_from_str("5/30/2022", "%m/%d/%Y").unwrap())));
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
        use std::fs::File;
        use std::io::Write;

        use super::*;
        use std::collections::btree_map::BTreeMap;
        use chrono::NaiveDate;
        use currency::Currency;


        //TODO add test case that errors thrown with malformed currency & dates
        //TODO add test case given some time & price filtering
        #[test]
        fn parse_sample() {
            let path = Path::new("test-resources/fda_calendar_sample_files/fda_calendar_sample.html");
            let actual = parse(path.to_str().unwrap());


            //TODO lots of to_string here. Rust code smell that we should be using more lifetimes?
            let expected_row = ParsedRow {
                price: Currency::from_str("$1.26").unwrap(),
                url: "https://www.biopharmcatalyst.com/company/BTX".to_string(),
                symbol: "BTX".to_string(),
                catalyst_date: NaiveDate::parse_from_str("2019-05-02", "%Y-%m-%d").unwrap(),
                drug_name: "OpRegen".to_string(),
                drug_indication: "Dry age-related macular degeneration (AMD)".to_string(),
                catalyst_note: "Phase 1/2 enrolment to be completed 2019. Updated data due  May 2, 2019,10:15am ET at ARVO.".to_string(),
                phase: "Phase 1/2".to_string(),
            };

            let mut catalysts = BTreeMap::new();
            catalysts.insert(("phase1.5".to_string(), NaiveDate::parse_from_str("2019-05-02", "%Y-%m-%d").unwrap()), vec![expected_row]);
            let expected = ScrapedCatalysts {
                catalysts
            };

            //TODO add another row with the same phase and date
            //TODO add a row with a different phase
            //Add range selection to demonstrate getting submap.

            assert_eq!(actual.unwrap(), expected);
        }

//        #[test]
//        #[ignore]
//        fn test() {
//            let path = Path::new("test-resources/fda_calendar_sample_files/fda_calendar_download.html");
//            parse(path.to_str().unwrap());
//        }
//        #[test]
//        #[ignore]
//        fn test2() {
//            //Demo code for downloading the document, for testing this was stored after downloading
//            let body = reqwest::get("https://www.biopharmcatalyst.com/calendars/fda-calendar").unwrap().text().unwrap();
//
//            let mut file = File::create("fda_calendar_download.html").unwrap();
//            file.write_all(body.as_bytes()).unwrap();
//            println!("body = {:?}", body);
//        }
    }
}