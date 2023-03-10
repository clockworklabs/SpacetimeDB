use sendgrid;
use sendgrid::v3::Email;
use sendgrid::v3::Personalization;
use sendgrid::v3::SGMap;

lazy_static::lazy_static! {
    pub static ref SENDGRID: SendGridController = SendGridController::new();
}

pub struct SendGridSettings {
    sender: sendgrid::v3::Sender,
    sendgrid_sender: String,
}

pub struct SendGridController {
    settings: Option<SendGridSettings>,
}

impl SendGridController {
    #[allow(clippy::new_without_default)]
    pub fn new() -> SendGridController {
        match (std::env::var("SENDGRID_API_KEY"), std::env::var("SENDGRID_SENDER")) {
            (Ok(api_key), Ok(sendgrid_sender)) => SendGridController {
                settings: Some(SendGridSettings {
                    sender: sendgrid::v3::Sender::new(api_key),
                    sendgrid_sender,
                }),
            },
            _ => {
                log::warn!("SENDGRID_API_KEY or SENDGRID_SENDER env variables not set, SendGrid is disabled.");
                SendGridController { settings: None }
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
        if let Some(settings) = &self.settings {
            let mut data = SGMap::new();
            data.insert(
                "link".to_string(),
                if link {
                    format!(
                        "https://spacetimedb.net/auth?email={}&code={}&identity={}",
                        email, code, identity
                    )
                } else {
                    format!("Your recovery code: {}", code)
                },
            );
            let personalization = Personalization::new(Email::new(email));
            let message = sendgrid::v3::Message::new(Email::new(&settings.sendgrid_sender))
                .set_subject("Login to SpacetimeDB")
                .set_template_id("d-70a7cea2bf8941a5909919fcf904d687")
                .add_personalization(personalization.add_dynamic_template_data(data));
            settings.sender.send(&message).await?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("SendGrid is disabled."))
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.settings.is_some()
    }
}
