#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::fs::File;
    use std::io::Write;

    //TODO we need to import the module for unit testing?
    use crate::fda_scraper::{parse};

    #[test]
    fn test() {
        let path = Path::new("test-resources/fda_calendar_sample_files/fda_calendar_download.html");
        parse(path.to_str().unwrap());
    }

    #[test]
    fn test2() {
        //Demo code for downloading the document, for testing this was stored after downloading
        let body = reqwest::get("https://www.biopharmcatalyst.com/calendars/fda-calendar").unwrap().text().unwrap();

        let mut file = File::create("fda_calendar_download.html").unwrap();
        file.write_all(body.as_bytes()).unwrap();
        println!("body = {:?}", body);
    }
}

pub mod fda_scraper {
    use std::fs;
    use scraper::{Html, Selector};

    pub fn parse(file_path: &str) {

        //TODO need to return this exception instead of expect-ing
        let contents = fs::read_to_string(file_path)
            .expect("Something went wrong reading the file");
        let document = Html::parse_document(&contents);


        let event_table_row_selector = Selector::parse("tr.js-tr.js-drug").unwrap();

        let price = Selector::parse("div[class=price]").unwrap();
        let symbol_and_url = Selector::parse("td a[href]").unwrap();
        let catalyst_date = Selector::parse("time[class=catalyst-date]").unwrap();
        let drug_name = Selector::parse("strong[class=drug]").unwrap();
        let drug_indication = Selector::parse("div[class=indication]").unwrap();
        let catalyst_note = Selector::parse("div[class=catalyst-note]").unwrap();
        let phase = Selector::parse("td.js-td--stage").unwrap();

        for an_event_table_row in document.select(&event_table_row_selector) {
            let price = an_event_table_row.select(&price).nth(0).unwrap().text().next().unwrap();
            let symbol = an_event_table_row.select(&symbol_and_url).nth(0).unwrap();
            let url = symbol.value().attr("href").unwrap();
            let symbol = symbol.text().next().unwrap();
            let catalyst_date = an_event_table_row.select(&catalyst_date).nth(0).unwrap().text().next().unwrap();
            let drug_name = an_event_table_row.select(&drug_name).nth(0).unwrap().text().next().unwrap();
            let drug_indication = an_event_table_row.select(&drug_indication).nth(0).unwrap().text().next().unwrap();
            let catalyst_note = an_event_table_row.select(&catalyst_note).nth(0).unwrap().text().next().unwrap();
            let phase = an_event_table_row.select(&phase).nth(0).unwrap().text().next().unwrap().trim();

            println!("Symbol: {} Price: {} Url: {} Date: {}, Drug: {}, Indication: {}, Catalyst: {}, Phase: {}", symbol, price, url, catalyst_date, drug_name, drug_indication, catalyst_note, phase);
        }
    }


}