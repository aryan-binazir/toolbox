package notify

func Send(title, body string) error {
	return platformNotify(title, body)
}

func PlaySound() {
	platformPlaySound()
}
