package main

import (
	"bytes"
	"fmt"
	"os"
	"reflect"

	"example.com/avrox-go/profile"
)

func main() {
	path := "../user-profiles.avro"
	if len(os.Args) > 1 {
		path = os.Args[1]
	}
	if err := run(path); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func run(path string) error {
	input, err := os.Open(path)
	if err != nil {
		return err
	}
	file, err := readAvroFile(input)
	closeErr := input.Close()
	if err != nil {
		return err
	}
	if closeErr != nil {
		return closeErr
	}

	for i, value := range file.profiles {
		referrer := "-"
		if value.ReferredBy != nil {
			referrer = value.ReferredBy.Username
		}
		fmt.Printf(
			"%2d. %s (%s) | status=%s role=%s | verified=%t | addresses=%d tags=%d | verification=%s | referred_by=%s\n",
			i+1,
			value.Username,
			value.FullName,
			value.Status,
			value.Role,
			value.EmailVerified,
			len(value.Addresses),
			len(value.Tags),
			verificationLabel(value.Verification),
			referrer,
		)
	}
	fmt.Printf("read %d user profiles from %s\n", len(file.profiles), path)

	var serialized bytes.Buffer
	if err := writeAvroFile(&serialized, file.schema, file.sync, file.profiles); err != nil {
		return err
	}
	roundtripped, err := readAvroFile(&serialized)
	if err != nil {
		return err
	}
	if !reflect.DeepEqual(roundtripped.profiles, file.profiles) {
		return fmt.Errorf("serialization mismatch: profiles differ after the round trip")
	}
	fmt.Printf(
		"wrote %d user profiles back to an in-memory Avro file and verified the round trip\n",
		len(file.profiles),
	)
	return nil
}

func verificationLabel(value profile.NullOrEmailVerificationOrPhoneVerificationOrString) string {
	switch {
	case value.Null != nil:
		return "none"
	case value.EmailVerification != nil:
		return fmt.Sprintf("email (%s)", value.EmailVerification.Email)
	case value.PhoneVerification != nil:
		return fmt.Sprintf("phone (%s)", value.PhoneVerification.PhoneNumber)
	case value.String != nil:
		return fmt.Sprintf("note (%s)", *value.String)
	default:
		return "invalid"
	}
}
