#include "ZuriComponent.h"

#include "Engine/Texture2D.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "ZuriFile.h"

UZuriComponent::UZuriComponent()
{
	PrimaryComponentTick.bCanEverTick = true;
}

void UZuriComponent::BeginPlay()
{
	Super::BeginPlay();

	Instance = File ? UZuriInstance::Create(this, File, Artboard) : nullptr;
	// An empty name plays the default, which a new instance is already doing.
	if (Instance)
	{
		Instance->bMuted = bMuted;
		Instance->Play(PlayName);
	}
}

void UZuriComponent::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
{
	Super::TickComponent(DeltaTime, TickType, ThisTickFunction);

	if (Instance)
	{
		Instance->SetFit(Fit);
		Instance->SetAlignment(Alignment);
		Instance->Advance(DeltaTime);
		Instance->Render(TextureSize.X, TextureSize.Y);
		if (Material)
		{
			// Does nothing unless the texture is a different object, which happens when TextureSize changes.
			Material->SetTextureParameterValue(TextureParameterName, Instance->GetTexture());
		}
	}
}
