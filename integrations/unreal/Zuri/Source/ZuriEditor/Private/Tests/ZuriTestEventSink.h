#pragma once

#include "CoreMinimal.h"
#include "UObject/Object.h"

#include "ZuriTestEventSink.generated.h"

/** Records the events an instance broadcasts, for the automation test. */
UCLASS()
class UZuriTestEventSink : public UObject
{
	GENERATED_BODY()

public:
	UFUNCTION()
	void HandleZuriEvent(FName EventName) { Events.Add(EventName); }

	TArray<FName> Events;
};
