use sendgrid::v3::{Email, Personalization, SGMap};

#[derive(Clone)]
pub struct SendGridController {
    sender: sendgrid::v3::Sender,
    sendgrid_sender: Email,
}

const AUTH_URL: &str = "https://spacetimedb.net/auth";

impl SendGridController {
    /// Get a SendGridController, pulling from environment variables, returning None if the env vars aren't present.
    pub fn new() -> Option<Self> {
        match (std::env::var("SENDGRID_API_KEY"), std::env::var("SENDGRID_SENDER")) {
            (Ok(api_key), Ok(sendgrid_sender)) => Some(SendGridController {
                sender: sendgrid::v3::Sender::new(api_key),
                sendgrid_sender: Email::new(sendgrid_sender),
            }),
            _ => {
                log::warn!("SENDGRID_API_KEY or SENDGRID_SENDER env variables not set, SendGrid is disabled.");
                None
            }
        }
    }

    /// # Description
    ///
    /// Sends a recovery email to the provided `email` address.
    ///
    /// # Arguments
    ///  * `email` - The email to send the recovery email to
    ///  * `code` - The recovery code to send
    ///  * `identity` - The identity the user is trying to recover
    ///  * `link` - Whether or not this request originated from the website. Typically if a user is
    ///               attempting to login to the website, we will send them a link to click on instead
    ///               of giving them a recovery code.
    pub async fn send_recovery_email(
        &self,
        email: &str,
        code: &str,
        identity: &str,
        link: bool,
    ) -> Result<(), anyhow::Error> {
        let mut data = SGMap::new();
        data.insert(
            "link".to_string(),
            if link {
                url::Url::parse_with_params(AUTH_URL, [("email", email), ("code", code), ("identity", identity)])
                    .unwrap()
                    .into()
            } else {
                format!("Your recovery code: {}", code)
            },
        );
        let personalization = Personalization::new(Email::new(email));
        let message = sendgrid::v3::Message::new(self.sendgrid_sender.clone())
            .set_subject("Login to SpacetimeDB")
            .set_template_id("d-70a7cea2bf8941a5909919fcf904d687")
            .add_personalization(personalization.add_dynamic_template_data(data));
        self.sender.send(&message).await?;
        Ok(())
    }
}
