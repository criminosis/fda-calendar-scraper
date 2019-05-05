pub mod fda_scraper {
    use std::fs;
    use scraper::{Html, Selector};
    use chrono::NaiveDate;
    use std::convert::From;
    use std::collections::BTreeMap;
    use currency::Currency;
    use log::{info, error};

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

    impl From<Html> for ScrapedCatalysts {
        fn from(document: Html) -> Self {
            let event_table_row_selector = Selector::parse("tr.js-tr.js-drug").unwrap();
            let price = Selector::parse("div[class=price]").unwrap();
            let symbol_and_url = Selector::parse("td a[href]").unwrap();
            let catalyst_date = Selector::parse("time[class=catalyst-date]").unwrap();
            let drug_name = Selector::parse("strong[class=drug]").unwrap();
            let drug_indication = Selector::parse("div[class=indication]").unwrap();
            let catalyst_note = Selector::parse("div[class=catalyst-note]").unwrap();
            let phase = Selector::parse("td.js-td--stage[data-value]").unwrap();

            let mut catalysts = BTreeMap::new();
            for an_event_table_row in document.select(&event_table_row_selector) {
                let price = an_event_table_row.select(&price).nth(0)
                    .and_then(|x| x.text().next())
                    .and_then(|x| Currency::from_str(x).ok())
                    .unwrap();

                let url_symbol_ref = an_event_table_row.select(&symbol_and_url).nth(0).unwrap();
                let url = url_symbol_ref.value().attr("href").map(|href_value| href_value.to_string()).unwrap();
                let symbol = url_symbol_ref.text().next().map(|symbol| symbol.to_string()).unwrap();

                let catalyst_date = an_event_table_row.select(&catalyst_date).nth(0)
                    .and_then(|x| x.text().next())
                    .and_then(|x| NaiveDate::parse_from_str(x, "%m/%d/%Y").ok())
                    .unwrap();

                let drug_name = an_event_table_row.select(&drug_name).nth(0)
                    .and_then(|x| x.text().next())
                    .map(|x| x.trim().to_string())
                    .unwrap();

                let drug_indication = an_event_table_row.select(&drug_indication).nth(0)
                    .and_then(|x| x.text().next())
                    .map(|x| x.to_lowercase())
                    .unwrap();

                let catalyst_note = an_event_table_row.select(&catalyst_note).nth(0)
                    .and_then(|x| x.text().next())
                    .map(|x| x.to_string())
                    .unwrap();

                let phase = an_event_table_row.select(&phase).nth(0).unwrap();
                let phase_grouping = phase.value().attr("data-value").map(|x| x.to_string()).unwrap();
                let phase = phase.text().next().map(|x| x.trim().to_string()).unwrap();


                let to_insert = ParsedRow { price, url, symbol, catalyst_date, drug_name, drug_indication, catalyst_note, phase };

                catalysts.entry((phase_grouping, catalyst_date)).or_insert(Vec::new()).push(to_insert);
            }

            ScrapedCatalysts {
                catalysts
            }
        }
    }

    pub fn parse(file_path: &str) -> Result<ScrapedCatalysts, std::io::Error> {
        let parsed: Result<ScrapedCatalysts, std::io::Error> = fs::read_to_string(file_path)
            .map(|contents| Html::parse_document(&contents))
            .map(|document| document.into());
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