package main

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
)

var logger = server.NewLogger("quickstart-chat")

//stdb:init
func initReducer(ctx server.ReducerContext) {
	// Called when the module is initially published
}

//stdb:connect
func identityConnected(ctx server.ReducerContext) {
	user, found, err := UserTable.FindByIdentity(ctx.Sender())
	if err != nil {
		panic(fmt.Sprintf("FindBy error: %v", err))
	}
	if found {
		user.Online = true
		UserTable.UpdateByIdentity(user)
	} else {
		UserTable.Insert(User{Identity: ctx.Sender(), Name: nil, Online: true})
	}
}

//stdb:disconnect
func identityDisconnected(ctx server.ReducerContext) {
	user, found, err := UserTable.FindByIdentity(ctx.Sender())
	if err != nil {
		panic(fmt.Sprintf("FindBy error: %v", err))
	}
	if found {
		user.Online = false
		UserTable.UpdateByIdentity(user)
	} else {
		logger.Warn(fmt.Sprintf("Disconnect event for unknown user with identity %v", ctx.Sender()))
	}
}

//stdb:reducer
func setName(ctx server.ReducerContext, name string) error {
	if name == "" {
		return fmt.Errorf("Names must not be empty")
	}
	user, found, err := UserTable.FindByIdentity(ctx.Sender())
	if err != nil {
		return fmt.Errorf("FindBy error: %w", err)
	}
	if !found {
		return fmt.Errorf("Cannot set name for unknown user")
	}
	logger.Info(fmt.Sprintf("User %v sets name to %s", ctx.Sender(), name))
	user.Name = &name
	UserTable.UpdateByIdentity(user)
	return nil
}

//stdb:reducer
func sendMessage(ctx server.ReducerContext, text string) error {
	if text == "" {
		return fmt.Errorf("Messages must not be empty")
	}
	logger.Info(fmt.Sprintf("User %v: %s", ctx.Sender(), text))
	MessageTable.Insert(Message{Sender: ctx.Sender(), Sent: ctx.Timestamp(), Text: text})
	return nil
}
