use std::{
    env,
    error::Error,
    fs::File,
    io::{BufReader, Cursor},
    path::{Path, PathBuf},
};

use apache_avro::{Reader, Writer, types::Value};

#[allow(dead_code)]
mod generated;

use generated::{NullOrEmailVerificationOrPhoneVerificationOrString as Verification, UserProfile};

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args().nth(1).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../user-profiles.avro"),
        PathBuf::from,
    );
    let reader = Reader::new(BufReader::new(File::open(&path)?))?;
    let schema = reader.writer_schema().clone();

    let mut profiles = Vec::new();
    for record in reader.into_deser_iter::<UserProfile>() {
        let profile = record?;
        let count = profiles.len() + 1;
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
        profiles.push(profile);
    }
    let count = profiles.len();
    println!("read {count} user profiles from {}", path.display());

    let original: Vec<Value> =
        Reader::new(BufReader::new(File::open(&path)?))?.collect::<Result<_, _>>()?;

    let mut writer = Writer::new(&schema, Vec::new())?;
    for profile in &profiles {
        writer.append_ser(profile)?;
    }
    let serialized = writer.into_inner()?;

    let roundtripped: Vec<Value> =
        Reader::new(Cursor::new(&serialized))?.collect::<Result<_, _>>()?;
    if roundtripped != original {
        let mismatch = original
            .iter()
            .zip(&roundtripped)
            .position(|(expected, actual)| expected != actual);
        return Err(match mismatch {
            Some(index) => format!(
                "serialization mismatch: record {} differs after the round trip\n\
                 expected: {:?}\nactual:   {:?}",
                index + 1,
                original[index],
                roundtripped[index],
            ),
            None => format!(
                "serialization mismatch: read back {} records, but expected {count}",
                roundtripped.len()
            ),
        }
        .into());
    }

    println!(
        "wrote {count} user profiles back to an in-memory Avro file and verified the round trip"
    );
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
