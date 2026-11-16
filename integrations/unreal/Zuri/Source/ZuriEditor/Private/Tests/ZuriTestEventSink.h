#pragma once

#include "CoreMinimal.h"
#include "UObject/Object.h"

#include "ZuriTestEventSink.generated.h"

class USoundWave;

/** Records the events and sounds an instance broadcasts, for the automation tests. */
UCLASS()
class UZuriTestEventSink : public UObject
{
	GENERATED_BODY()

public:
	UFUNCTION()
	void HandleZuriEvent(FName EventName) { Events.Add(EventName); }

	UFUNCTION()
	void HandleZuriSound(USoundWave* Sound, float Volume) { Sounds.Emplace(Sound, Volume); }

	TArray<FName> Events;
	TArray<TPair<USoundWave*, float>> Sounds;
};
