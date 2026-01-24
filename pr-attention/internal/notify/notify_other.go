//go:build !linux && !darwin

package notify

import "fmt"

func platformNotify(title, body string) error {
	// Unsupported platform - just print to stdout
	fmt.Printf("[NOTIFICATION] %s: %s\n", title, body)
	return nil
}

func platformPlaySound() {
	// Terminal bell fallback
	fmt.Print("\a")
}
