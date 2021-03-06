use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use reqwest;
use serde::{Deserialize, Serialize};
use serde_json as json;
use serde_urlencoded as url_encode;

/// Creating a custom error for mapping Errors to return result from the library handles
/// The possible errors are `URLEncodeFailure`, `URLDecodeFailure`, `HTTPRequestError`, and `NotDelivered`
/// `URLDecodeFailure` maps to a `serde_json::error::Error`
/// `URLEncodeFailure` maps to a `serde_urlencoded::ser::Error`
/// `HTTPRequestError` maps to a `reqwest::error::Error`
/// `NotDelivered` is a custom error that is sent when an SMS was not delivered
#[derive(Debug)]
pub enum TWRSError {
    URLEncodeFailure(serde_urlencoded::ser::Error),
    URLDecodeFailure(serde_json::error::Error),
    HTTPRequestError(reqwest::Error),
    NotDelivered(String),
}

impl fmt::Display for TWRSError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TWRSError::URLEncodeFailure(e) => {
                write!(f, "Error while serializing URL to encoded string: {}", e)
            }
            TWRSError::URLDecodeFailure(e) => {
                write!(f, "Error while serializing URL to encoded string: {}", e)
            }
            TWRSError::HTTPRequestError(e) => write!(f, "Error while sending HTTP POST: {}", e),
            TWRSError::NotDelivered(e) => write!(f, "Error message not delivered: {}", e),
        }
    }
}

impl Error for TWRSError {}

/// Custom struct to serialize the HTTP POST data into a url encoded objecting using serde_urlencoded
/// For a description of these fields see the [Official Twilio Developer Documentation](https://www.twilio.com/docs/sms)
/// All fields must exist so none of them is given the Serde ignore on None tag
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize)]
pub struct TwilioSend<'s> {
    pub Body: &'s str,
    pub r#From: &'s str,
    pub To: &'s str,
}

/// Creates a new instance of the body that is posted to the Twilio API
impl<'s> TwilioSend<'s> {
    pub fn new() -> TwilioSend<'s> {
        TwilioSend {
            r#From: "",
            To: "",
            Body: "",
        }
    }

    /// This function converts from the struct to a string of url encoded formatting
    pub fn encode(self) -> Result<String, TWRSError> {
        url_encode::to_string(&self).map_err(TWRSError::URLEncodeFailure)
    }
}

/// Struct to deserialize the Twilio reply from the post to the API
/// This is used to inspect the response to ensure the message was delivered
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct TwilioReply {
    sid: String,
    date_created: String,
    date_updated: String,
    date_sent: Option<String>,
    account_sid: String,
    to: String,
    from: String,
    messaging_service_sid: Option<String>,
    body: String,
    status: String,
    num_segments: String,
    num_media: String,
    direction: String,
    api_version: String,
    price: Option<String>,
    price_unit: String,
    error_code: Option<String>,
    error_message: Option<String>,
    uri: String,
    subresource_uris: HashMap<String, String>,
}

impl TwilioReply {
    /// Deserialize the response from the Twilio API directly from the `reqwest::blocking::Response`
    /// struct
    pub fn decode(response: &mut reqwest::blocking::Response) -> Result<TwilioReply, TWRSError> {
        let mut buf: Vec<u8> = Vec::new();
        response
            .copy_to(&mut buf)
            .expect("Error copying bytes to String buffer");
        let str_t = String::from_utf8(buf).expect("Error decoding as UTF-8 from Response");

        json::from_str(&str_t).map_err(TWRSError::URLDecodeFailure)
    }

    /// Deserialize the response from a `&str`
    pub fn decode_str(response: &str) -> Result<TwilioReply, serde_json::error::Error> {
        json::from_str(&response)
    }
}

/// Main function of the library which sends the request and returns the response
/// response. Will error out on a `TWRSError::HTTPRequestError` if the send results in a failure
pub fn send_message(
    account_sid: &str,
    auth_token: &str,
    body: String,
) -> Result<reqwest::blocking::Response, TWRSError> {
    let endpoint = "https://api.twilio.com/2010-04-01/Accounts".to_string();
    let uri = format!("{}/{}/Messages.json", endpoint, account_sid);

    reqwest::blocking::Client::new()
        .post(&uri)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .basic_auth(account_sid, Some(auth_token))
        .body(body)
        .send()
        .map_err(TWRSError::HTTPRequestError)
}
/// This will check if the status is set to delivered within the Twilio API
/// Within this function is a while loop that breaks on the API returning anything other than
/// `delivered`, if the response is not delivered this will return `TWRSError::NotDelivered`
pub fn is_delivered<'r>(
    response: &mut reqwest::blocking::Response,
    account_sid: &str,
    auth_token: &str,
) -> Result<&'r str, TWRSError> {
    let resp_body = TwilioReply::decode(response).expect("Error decoding response");
    let mut resp_status = resp_body.status;
    let url = format!("https://api.twilio.com/{}", resp_body.uri);

    while resp_status == "queued" || resp_status == "sent" {
        let mut sub_r = reqwest::blocking::Client::new()
            .get(&url)
            .basic_auth(account_sid, Some(auth_token))
            .send()
            .expect("Error sending response inspector get request");
        let sub_res = TwilioReply::decode(&mut sub_r).expect("Error decoding response from server");
        resp_status = sub_res.status;
    }

    match resp_status.as_ref() {
        "delivered" => Ok("delivered"),
        _ => Err(TWRSError::NotDelivered(resp_status)),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_encoding() {
        use crate as twrs_sms;

        let mut tw = twrs_sms::TwilioSend::new();
        tw.From = "+11234567890";
        tw.To = "+10987654321";
        tw.Body = "Hello, world!";

        let tw_e = tw.encode().expect("Error converting to url encoded scheme");

        assert_eq!(
            tw_e,
            "Body=Hello%2C+world%21&From=%2B11234567890&To=%2B10987654321".to_string()
        );
    }

    #[test]
    fn test_decoding() {
        use crate as twrs_sms;

        let d = "{\"sid\": \"XXXX\", \"date_created\": \"Wed, 22 Jan 2020 15:23:30 +0000\", \"date_updated\": \"Wed, 22 Jan 2020 15:23:30 +0000\", \"date_sent\": null, \"account_sid\": \"ACXXXX\", \"to\": \"+11234567890\", \"from\": \"+10987654321\", \"messaging_service_sid\": null, \"body\": \"Sent from your Twilio trial account - Hiya\", \"status\": \"queued\", \"num_segments\": \"1\", \"num_media\": \"0\", \"direction\": \"outbound-api\", \"api_version\": \"2010-04-01\", \"price\": null, \"price_unit\": \"USD\", \"error_code\": null, \"error_message\": null, \"uri\": \"/2010-04-01/Accounts/ACXXXX/Messages/XXXX.json\", \"subresource_uris\": {\"media\": \"/2010-04-01/Accounts/ACXXXX/Messages/XXXX/Media.json\"}}".to_string();

        let t_r = twrs_sms::TwilioReply::decode_str(&d).expect("Error decoding reply");

        let expected: twrs_sms::TwilioReply = twrs_sms::TwilioReply {
            sid: "XXXX".to_string(),
            date_created: "Wed, 22 Jan 2020 15:23:30 +0000".to_string(),
            date_updated: "Wed, 22 Jan 2020 15:23:30 +0000".to_string(),
            date_sent: None,
            account_sid: "ACXXXX".to_string(),
            to: "+11234567890".to_string(),
            from: "+10987654321".to_string(),
            messaging_service_sid: None,
            body: "Sent from your Twilio trial account - Hiya".to_string(),
            status: "queued".to_string(),
            num_segments: "1".to_string(),
            num_media: "0".to_string(),
            direction: "outbound-api".to_string(),
            api_version: "2010-04-01".to_string(),
            price: None,
            price_unit: "USD".to_string(),
            error_code: None,
            error_message: None,
            uri: "/2010-04-01/Accounts/ACXXXX/Messages/XXXX.json".to_string(),
            subresource_uris: {
                [(
                    "media".to_string(),
                    "/2010-04-01/Accounts/ACXXXX/Messages/XXXX/Media.json".to_string(),
                )]
                .iter()
                .cloned()
                .collect()
            },
        };

        assert_eq!(t_r, expected);
    }

    #[test]
    #[ignore]
    fn test_full() {
        // Be sure to have the follow environment variables set before running this ignored test
        // export TW_TO="COUNTRYCODE_PHONENUMBER"
        // export TW_FROM="COUNTRYCODE_PHONENUMBER"
        // export TW_SID="ACCOUNT_SID"
        // export TW_TOKEN="ACCOUNT_TOKEN"
        use crate as twrs_sms;
        use std::env::var;

        use reqwest::StatusCode;

        // Getting your Twilio info to test sending an SMS
        let tw_to = var("TW_TO").unwrap();
        let tw_from = var("TW_FROM").unwrap();
        let tw_sid = var("TW_SID").unwrap();
        let tw_token = var("TW_TOKEN").unwrap();

        // Create the request body and encode the message for the API
        let t: twrs_sms::TwilioSend = twrs_sms::TwilioSend {
            To: &tw_to,
            From: &tw_from,
            Body: "Hiya",
        };
        let t_s = t.encode().expect("Error converting to url encoded string");

        // Send the message to the API endpoint
        let mut response =
            twrs_sms::send_message(&tw_sid, &tw_token, t_s).expect("Error with HTTP request");

        // Server responds with 201 (Created) on the initial response
        assert_eq!(StatusCode::from_u16(201).unwrap(), response.status());

        // Run the loop to make sure the message was delivered
        let delivered = twrs_sms::is_delivered(&mut response, &tw_sid, &tw_token)
            .expect("Error SMS not delivered");

        // Checking the delivered state, and fail on an error
        assert_eq!(delivered, "delivered");
    }
}
