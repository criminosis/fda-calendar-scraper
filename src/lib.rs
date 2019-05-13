mod currency; //Declares that we have a module called currency in file currency.rs in src/

pub mod fda_scraper {
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

    #[derive(Debug, Eq, PartialEq)]
    struct ParsedRow {
        price: currency::USD,
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
        CurrencyParseError(currency::USDParseError),
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

    //New type to make sure only PhaseLabels are used as the key type in ScrapedCatalysts. Zero cost abstraction
    #[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
    struct PhaseLabel(String);

    #[derive(Debug, Eq, PartialEq)]
    pub struct ScrapedCatalysts {
        //TODO convert the string to PhaseLabel new type?
        catalysts: BTreeMap<(PhaseLabel, NaiveDate), Vec<ParsedRow>>
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
        pub fn new(document: &Html, scrape_predicate: (&Fn(&currency::USD) -> bool, &Fn(&NaiveDate) -> bool)) -> Result<ScrapedCatalysts, ScrapeError> {

            let (price_limit, date_limit) = scrape_predicate;

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
                    .and_then(|x| currency::USD::new(&x)
                        .map_err(|x| ScrapeError::CurrencyParseError(x)))?;

                let catalyst_date = select_first_text_from(&an_event_table_row, &catalyst_date)
                    .and_then(|x| NaiveDate::parse_from_str(x, "%m/%d/%Y")
                        .map_err(|x| ScrapeError::DateParseFailure(x)))?;

                if !price_limit(&price) || !date_limit(&catalyst_date) {
                    continue;
                }


                let url_symbol_ref = select_first_element_from(&an_event_table_row, &symbol_and_url)
                    .ok_or_else(||ScrapeError::ExpectedFieldNotFound(symbol_and_url.clone()))?;
                let url = retrieve_attr_from(&url_symbol_ref, "href", &symbol_and_url)?;
                let symbol = retrieve_text_from(&url_symbol_ref, &symbol_and_url)?;

                let drug_name = select_first_string(&an_event_table_row, &drug_name)?;
                let drug_indication = select_first_string(&an_event_table_row, &drug_indication)?;
                let catalyst_note = select_first_string(&an_event_table_row, &catalyst_note)?;


                let phase_element = select_first_element_from(&an_event_table_row, &phase)
                    .ok_or_else(|| ScrapeError::ExpectedFieldNotFound(phase.clone()))?;
                let phase_grouping = PhaseLabel(retrieve_attr_from(&phase_element, "data-value", &phase)?);
                let phase = retrieve_text_from(&phase_element, &phase)?;

                let to_insert = ParsedRow { price, url, symbol, catalyst_date, drug_name, drug_indication, catalyst_note, phase };

                catalysts.entry((phase_grouping, catalyst_date)).or_insert(Vec::new()).push(to_insert);
            }

            Ok(ScrapedCatalysts { catalysts })
        }
    }

    pub fn parse_all_rows(file_path: &Path) -> Result<ScrapedCatalysts, ScrapeError> {
        //Just return everything
        parse_rows_with_predicates(file_path, &|_:&currency::USD| true, &|_:&NaiveDate| true)
    }

    pub fn parse_rows_with_predicates(file_path: &Path,
                 price_predicate: &Fn(&currency::USD) -> bool, date_predicate: &Fn(&NaiveDate) -> bool) -> Result<ScrapedCatalysts, ScrapeError> {
        let parsed = fs::read_to_string(file_path)
            .map_err(|x| ScrapeError::FileReadError(x))
            .map(|contents| Html::parse_document(&contents))
            .and_then(|document| ScrapedCatalysts::new(&document, (price_predicate, date_predicate)));
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
            match parse_all_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_malformed_date.html")) {
                Err(ScrapeError::DateParseFailure(_)) => panic!("Failed to parse date"),
                x => panic!("Unexpected result {:?}", x)
            }
        }

        #[test]
        #[should_panic(expected = "Failed to parse currency")]
        fn parse_malformed_price() {
            match parse_all_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_malformed_price.html")) {
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
                       parse_all_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_sample.html")).unwrap());
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

            assert_eq!(ScrapedCatalysts { catalysts }, parse_all_rows(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_multiple_rows.html")).unwrap());
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

            let price_limit = &currency::USD::new("$6").unwrap();
            assert_eq!(ScrapedCatalysts { catalysts },
                       parse_rows_with_predicates(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_multiple_rows.html"),
                                                  &|x| x <= price_limit,
                                                  &|_| true).unwrap());
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

            let date_limit = &NaiveDate::parse_from_str("2019-05-03", "%Y-%m-%d").unwrap();
            assert_eq!(ScrapedCatalysts { catalysts },
                       parse_rows_with_predicates(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_multiple_rows.html"),
                                                  &|_| true,
                                                  &|x| x < date_limit).unwrap());
        }

        #[test]
        fn parse_with_price_and_date_ceiling() {
            let price_limit = &currency::USD::new("$1").unwrap();
            let date_limit = &NaiveDate::parse_from_str("2019-05-03", "%Y-%m-%d").unwrap();
            assert_eq!(ScrapedCatalysts { catalysts: BTreeMap::new()},
                       parse_rows_with_predicates(Path::new("test-resources/fda_calendar_sample_files/fda_calendar_multiple_rows.html"),
                                                  &|x| x <= price_limit,
                                                  &|x| x < date_limit).unwrap());
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