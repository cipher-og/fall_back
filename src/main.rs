use chrono::{Datelike, Duration, Local, Weekday};
use log::{info, warn};
use reqwest::Error;
use scraper::{Html, Selector};
use serde_json::json;
use tokio;

struct Range{
    high:f64,
    low:f64
}

impl Range{
    const STRIKE_OFFSET: f64 = 50.0;
    const STRIKE_ROUND: f64 = 100.0;
    const NIFTY_EXPIRY: Weekday = Weekday::Thu;

    fn new(high:f64,low:f64) -> Self{
        Self{
            high,
            low
        }
    }


    fn strike(&self, ce: bool) -> f64 {
        let offset = if ce {
            self.high-Self::STRIKE_OFFSET
        } else {
            self.low+Self::STRIKE_OFFSET
        };
        
        (offset/Self::STRIKE_ROUND).round()*Self::STRIKE_ROUND
    }

    


    fn expiry_day(&self) -> String{

        let mut expiry_day = Local::now();
        while expiry_day.weekday() != Self::NIFTY_EXPIRY{
            expiry_day += Duration::days(1);

        }
        expiry_day.format("_%d%b%Y_").to_string().to_uppercase()
    }

    fn instrument(&self, ce: bool) -> (String,String) {
        let strike_price = self.strike(ce).to_string();
        let encode = if ce {
            format!("CE_{}",strike_price)
        } else {
            format!("PE_{}",strike_price)
        };
        
        (format!("OPTIDX_NIFTY{}{}",self.expiry_day(),encode).to_string(),strike_price)
    }

}


async fn fetch_html(url: &str) -> Option<String> {
    let response = reqwest::get(url).await.ok()?;
    if response.status().is_success() {
        Some(response.text().await.ok()?)
    } else {
        warn!("Failed to fetch URL: {}", url);
        None
    }
}

async fn scrape_nifty_price_range() -> Option<Vec<String>> {
    let url = "https://www.google.com/finance/quote/NIFTY_50:INDEXNSE?hl=en";
    let html = fetch_html(url).await?;
    let document = Html::parse_document(&html);
    let selector = Selector::parse("div.P6K39c").ok()?;
    println!("{:?}",selector);
    let mut price_divs = document.select(&selector);

    // Assuming the price range is in the second matching div
    let price_div = price_divs.nth(1)?;
    let range_text = price_div.text().collect::<Vec<_>>().concat();

    let range = range_text
        .trim()
        .replace(",", "")
        .split(" - ")
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    Some(range)
}

async fn push_payload(instr_key: &str, key_value: &str) -> Result<(), Error> {
    let auth_token = "-";
    let payload = json!({
        "auth-token": auth_token,
        "key": instr_key,
        "value": key_value
    });

    let client = reqwest::Client::new();
    
    let response = client
        .post("https://api.tradetron.tech/api?")
        .json(&payload)
        .send()
        .await?;

    Ok(())
}

async fn process_nifty_range() -> Result<(String, String, String, String), Box<dyn std::error::Error>> {
    if let Some(range) = scrape_nifty_price_range().await {
        if range.len() == 2 {
            if let (Ok(high), Ok(low)) = (range[0].parse::<f64>(), range[1].parse::<f64>()) {
                let range_instance = Range::new(high, low);
                
                let (ce_instru, ce_strike) = range_instance.instrument(true);
                let (pe_instru, pe_strike) = range_instance.instrument(false);
                
                push_payload("ce_strike", &ce_strike).await?;
                push_payload("pe_strike", &pe_strike).await?;
                push_payload("ce_instru", &ce_instru).await?;
                push_payload("pe_instru", &pe_instru).await?;

                return Ok((ce_strike, pe_strike, ce_instru, pe_instru));
            }
        }
    }
    Err("Failed to process NIFTY range.".into())
}

#[tokio::main]
async fn main() {

    env_logger::init();
    match process_nifty_range().await {
        Ok((ce_strike, pe_strike, pe_instru, ce_instru)) => {
            info!("Call Strike: {}", ce_strike);
            info!("Put Strike: {}", pe_strike);
            info!("Put Instrument: {}", pe_instru);
            info!("Put Instrument: {}", ce_instru);

        }
        Err(e) => {
            warn!("Error processing NIFTY range: {}", e);
        }
    }
}
