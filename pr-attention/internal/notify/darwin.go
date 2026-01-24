//go:build darwin

package notify

import (
	"fmt"
	"os/exec"
)

func platformNotify(title, body string) error {
	script := fmt.Sprintf(`display notification %q with title %q`, body, title)
	cmd := exec.Command("osascript", "-e", script)
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("osascript notification failed: %w", err)
	}
	return nil
}

func platformPlaySound() {
	exec.Command("afplay", "/System/Library/Sounds/Ping.aiff").Run()
}
