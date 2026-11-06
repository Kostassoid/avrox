use std::{
    env,
    error::Error,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use apache_avro::Reader;

#[allow(dead_code)]
mod generated;

use generated::{NullOrEmailVerificationOrPhoneVerificationOrString as Verification, UserProfile};

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args().nth(1).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../user-profiles.avro"),
        PathBuf::from,
    );
    let reader = Reader::new(BufReader::new(File::open(&path)?))?;

    let mut count = 0;
    for record in reader.into_deser_iter::<UserProfile>() {
        let profile = record?;
        count += 1;
        let referrer = profile
            .referred_by
            .as_deref()
            .map_or("-", |referrer| referrer.username.as_str());
        println!(
            "{count:>2}. {} ({}) | status={:?} role={:?} | verified={} | \
             addresses={} tags={} | verification={} | referred_by={referrer}",
            profile.username,
            profile.full_name,
            profile.status,
            profile.role,
            profile.email_verified,
            profile.addresses.len(),
            profile.tags.len(),
            verification_label(&profile.verification),
        );
    }

    println!("read {count} user profiles from {}", path.display());
    Ok(())
}

fn verification_label(verification: &Verification) -> String {
    match verification {
        Verification::Null(()) => "none".to_owned(),
        Verification::EmailVerification(verification) => {
            format!("email ({})", verification.email)
        }
        Verification::PhoneVerification(verification) => {
            format!("phone ({})", verification.phone_number)
        }
        Verification::String(note) => format!("note ({note})"),
    }
}
