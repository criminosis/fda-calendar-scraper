use std::{fmt, error};
use crate::currency::USDParseError::{NoDollarSign, InvalidStructure, DecimalWithInsufficientCents};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct USD {
    is_positive: bool,
    cents: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum USDParseError {
    //TODO should these just be &`a str? So that way error msgs just need a long lived reference?
    NoDollarSign(String),
    DecimalWithInsufficientCents(String),
    InvalidStructure(String),
}


impl USD {
    pub fn new(usd_original: &str) -> Result<USD, USDParseError> {

        let mut usd = usd_original;

        let is_positive = match usd.chars().nth(0) {
            Some('-') => {
                usd = &usd[1..]; //move past this character
                false
            },
            _ => true
        };


        match usd.chars().nth(0) {
            Some('$') => {
                usd = &usd[1..]; //move past this character
            },
            _ => return Err(NoDollarSign(usd_original.to_string()))
        };

        let mut split: Vec<&str> = usd.split(".").collect();
        split.reverse(); //it's a vector so we need to flip it around
        println!("post split {:?}", split);

        let dollars = match split.pop() {
            Some(x) => x,
            _ => return Err(InvalidStructure(usd_original.to_string()))
        };

        println!("post pop {:?}", split);

        let dollars: Result<u64, core::num::ParseIntError> = (if dollars.len() == 0 {"0"} else {dollars}) .parse();

        let mut cents_sum = match dollars {
            Ok(x) => x * 100,
            Err(_) => return Err(InvalidStructure(usd_original.to_string()))
        };

        let cents = split.pop().map(|cents_original| {
            println!("parsing {}", cents_original);
            if cents_original.len() != 2 {
                return Err(DecimalWithInsufficientCents(usd_original.to_string()))
            }
            let cents: Result<u64, core::num::ParseIntError> = cents_original.parse();
            cents.map_err(|_| InvalidStructure(usd_original.to_string()))
        });

        if cents.is_some() {
            cents_sum += cents.unwrap()?;
        };

        if split.is_empty() {
            Ok(USD {is_positive, cents: cents_sum})
        }
        else {
            //We had a 2nd decimal position, this is invalid
            Err(InvalidStructure(usd_original.to_string()))
        }
    }
}

impl error::Error for USDParseError {
    fn cause(&self) -> Option<&error::Error> {
        match self {
            USDParseError::DecimalWithInsufficientCents(_) => None,
            USDParseError::InvalidStructure(_) => None,
            USDParseError::NoDollarSign(_) => None
        }
    }
}

impl fmt::Display for USDParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            USDParseError::DecimalWithInsufficientCents(malformed_input) => write!(f, "Not enough decimal places {}", malformed_input),
            USDParseError::InvalidStructure(malformed_input) => write!(f, "Invalid structure {}", malformed_input),
            USDParseError::NoDollarSign(malformed_input) => write!(f, "No dollar sign {}", malformed_input),
        }
    }
}

impl fmt::Display for USD {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let sign = if self.is_positive {""} else {"-"};
        let dollars = self.cents / 100;
        let cents = self.cents % 100;
        write!(fmt, "{}${:01}.{:02}", sign, dollars, cents)
    }
}

#[cfg(test)]
mod tests {
use super::*;

    #[test]
    fn simple_parse() {
        let actual = USD::new("$1.01").unwrap();
        assert_eq!(actual.cents, 101);
        assert_eq!(actual.is_positive, true);
    }

    #[test]
    fn simple_negative_parse() {
        let actual = USD::new("-$1.01").unwrap();
        assert_eq!(actual.cents, 101);
        assert_eq!(actual.is_positive, false);
    }

    #[test]
    fn no_dollar_sign() {
        assert_eq!(USD::new("1.01").err().unwrap(), USDParseError::NoDollarSign("1.01".to_string()));
    }

    #[test]
    fn negative_no_dollar_sign() {
        assert_eq!(USD::new("-1.01").err().unwrap(), USDParseError::NoDollarSign("-1.01".to_string()));
    }

    #[test]
    fn non_dollar_sign() {
        assert_eq!(USD::new("%1.01").err().unwrap(), USDParseError::NoDollarSign("%1.01".to_string()));
    }

    #[test]
    fn no_decimal() {
        let actual = USD::new("$101").unwrap();
        assert_eq!(actual.cents, 10100);
        assert_eq!(actual.is_positive, true);
    }

    #[test]
    fn no_decimal_negative() {
        let actual = USD::new("-$101").unwrap();
        assert_eq!(actual.cents, 10100);
        assert_eq!(actual.is_positive, false);
    }

    #[test]
    fn no_dollars() {
        let actual = USD::new("$.05").unwrap();
        assert_eq!(actual.cents, 5);
        assert_eq!(actual.is_positive, true);
    }

    #[test]
    fn no_dollars_negative() {
        let actual = USD::new("-$.05").unwrap();
        assert_eq!(actual.cents, 5);
        assert_eq!(actual.is_positive, false);
    }

    #[test]
    fn decimal_no_cents() {
        assert_eq!(USD::new("$1.").err().unwrap(), USDParseError::DecimalWithInsufficientCents("$1.".to_string()));
    }

    #[test]
    fn string_loop() {
        let original = USD::new("-$.05").unwrap();
        let intermediary_string = original.to_string();
        assert_eq!(intermediary_string, "-$0.05".to_string());
        assert_eq!(USD::new(&intermediary_string).unwrap(), original);
    }

    #[test]
    fn non_numeric_dollars() {
        assert_eq!(USD::new("$A.44").err().unwrap(), USDParseError::InvalidStructure("$A.44".to_string()));
    }

    #[test]
    fn non_numeric_dollars_negative() {
        assert_eq!(USD::new("-$A.44").err().unwrap(), USDParseError::InvalidStructure("-$A.44".to_string()));
    }

    #[test]
    fn non_numeric_cents() {
        assert_eq!(USD::new("$4.AA").err().unwrap(), USDParseError::InvalidStructure("$4.AA".to_string()));
    }

    #[test]
    fn non_numeric_cents_negative() {
        assert_eq!(USD::new("-$4.AA").err().unwrap(), USDParseError::InvalidStructure("-$4.AA".to_string()));
    }

    #[test]
    fn insufficient_cents() {
        assert_eq!(USD::new("$1.0").err().unwrap(), USDParseError::DecimalWithInsufficientCents("$1.0".to_string()));
    }

    #[test]
    fn insufficient_cents_negative() {
        assert_eq!(USD::new("-$1.0").err().unwrap(), USDParseError::DecimalWithInsufficientCents("-$1.0".to_string()));
    }

    #[test]
    fn invalid_structure() {
        assert_eq!(USD::new("$1.00.33").err().unwrap(), USDParseError::InvalidStructure("$1.00.33".to_string()));
    }

    #[test]
    fn comparison() {
        let negative = USD::new("-$.05").unwrap();
        let zero = USD::new("$0").unwrap();
        let positive = USD::new("$1").unwrap();

        assert!(negative < zero);
        assert!(zero < positive);
        assert!(negative < positive)
    }

}