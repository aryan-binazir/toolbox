//go:build linux

package notify

import (
	"fmt"
	"os"
	"os/exec"
)

func platformNotify(title, body string) error {
	cmd := exec.Command("notify-send", title, body)
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("notify-send failed: %w", err)
	}
	return nil
}

func platformPlaySound() {
	soundPath := "/usr/share/sounds/freedesktop/stereo/message-new-instant.oga"
	if _, err := os.Stat(soundPath); err == nil {
		exec.Command("paplay", soundPath).Run()
		return
	}
	// Fallback to terminal bell
	fmt.Print("\a")
}
