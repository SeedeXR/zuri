#include "ZuriInstance.h"

#include "Engine/Texture2D.h"
#include "RHITypes.h"
#include "Templates/SharedPointer.h"
#include "TextureResource.h"
#include "UObject/Package.h"
#include "ZuriFile.h"
#include "ZuriRuntime.h"

// The enums are passed to the runtime as they are.
static_assert(static_cast<uint32>(EZuriFit::Contain) == ZURI_FIT_CONTAIN && static_cast<uint32>(EZuriFit::None) == ZURI_FIT_NONE, "EZuriFit must match ZURI_FIT_*");
static_assert(static_cast<uint32>(EZuriPropertyType::Number) == ZURI_NUMBER && static_cast<uint32>(EZuriPropertyType::String) == ZURI_STRING, "EZuriPropertyType must match the ZURI_* property types");

/** The largest render target side the runtime accepts. */
static constexpr int32 MaxRenderSize = 16384;

void FZuriInstanceDeleter::operator()(ZuriInstance* Handle) const
{
	zuri_instance_free(Handle);
}

UZuriInstance* UZuriInstance::Create(UObject* Outer, UZuriFile* File, const FString& Artboard)
{
	int32 ArtboardIndex = 0;
	if (File && !Artboard.IsEmpty())
	{
		ArtboardIndex = Zuri::FindName(File->ArtboardNames, Artboard);
		if (ArtboardIndex == INDEX_NONE)
		{
			UE_LOG(LogZuri, Error, TEXT("%s has no artboard called '%s'."), *File->GetName(), *Artboard);
			return nullptr;
		}
	}
	return CreateByIndex(Outer, File, ArtboardIndex);
}

UZuriInstance* UZuriInstance::CreateByIndex(UObject* Outer, UZuriFile* File, int32 ArtboardIndex)
{
	// A negative index wraps to one the runtime rejects.
	ZuriInstance* NewHandle = zuri_instance_new(File ? File->GetHandle() : nullptr, static_cast<uint32>(ArtboardIndex));
	if (!NewHandle)
	{
		UE_LOG(LogZuri, Error, TEXT("Cannot create an instance of %s: %s"), *GetNameSafe(File), *Zuri::LastError());
		return nullptr;
	}

	UZuriInstance* Instance = NewObject<UZuriInstance>(Outer ? Outer : GetTransientPackage());
	Instance->Handle.Reset(NewHandle);
	Instance->File = File;
	zuri_instance_autoplay(NewHandle);
	return Instance;
}

void UZuriInstance::BeginDestroy()
{
	// Here rather than in the destructor, which may run off the game thread.
	Handle.Reset();
	Super::BeginDestroy();
}

FVector2D UZuriInstance::GetArtboardSize() const
{
	return FVector2D(zuri_instance_width(Handle.Get()), zuri_instance_height(Handle.Get()));
}

TArray<FString> UZuriInstance::GetStateMachineNames() const
{
	return Zuri::ReadNames(Handle.Get(), zuri_instance_state_machine_count(Handle.Get()), zuri_instance_state_machine_name);
}

TArray<FString> UZuriInstance::GetAnimationNames() const
{
	return Zuri::ReadNames(Handle.Get(), zuri_instance_animation_count(Handle.Get()), zuri_instance_animation_name);
}

bool UZuriInstance::Play(const FString& Name)
{
	if (Name.IsEmpty())
	{
		zuri_instance_autoplay(Handle.Get());
		return true;
	}

	// INDEX_NONE wraps to an index the runtime rejects.
	const bool bPlaying = zuri_instance_play_state_machine(Handle.Get(), static_cast<uint32>(Zuri::FindName(GetStateMachineNames(), Name)))
		|| zuri_instance_play_animation(Handle.Get(), static_cast<uint32>(Zuri::FindName(GetAnimationNames(), Name)));
	if (!bPlaying)
	{
		UE_LOG(LogZuri, Warning, TEXT("%s has no state machine or animation called '%s'."), *GetNameSafe(File), *Name);
	}
	return bPlaying;
}

bool UZuriInstance::Advance(float DeltaSeconds)
{
	const bool bChanged = zuri_instance_advance(Handle.Get(), DeltaSeconds);
	bDirty |= bChanged;

	// Read every name before broadcasting: a handler may call back into this instance.
	const TArray<FString> Events = Zuri::ReadNames(Handle.Get(), zuri_instance_poll_events(Handle.Get()), zuri_instance_event_name);
	for (const FString& Event : Events)
	{
		OnZuriEvent.Broadcast(FName(*Event));
	}
	return bChanged;
}

void UZuriInstance::SetFit(EZuriFit InFit)
{
	bDirty |= Fit != InFit;
	Fit = InFit;
}

bool UZuriInstance::Render(int32 Width, int32 Height)
{
	const FIntPoint Size(FMath::Min(Width, MaxRenderSize), FMath::Min(Height, MaxRenderSize));
	const bool bResized = Size != RenderSize;
	if (!Handle || Size.X <= 0 || Size.Y <= 0 || (!bDirty && !bResized))
	{
		return false;
	}

	Pixels.SetNumUninitialized(Size.X * Size.Y * 4);
	if (!zuri_instance_render(Handle.Get(), Pixels.GetData(), static_cast<uint32>(Size.X), static_cast<uint32>(Size.Y), static_cast<uint32>(Fit), 0, false))
	{
		UE_LOG(LogZuri, Error, TEXT("Cannot render %s: %s"), *GetNameSafe(File), *Zuri::LastError());
		Pixels.Reset();
		RenderSize = FIntPoint::ZeroValue;
		return false;
	}
	RenderSize = Size;
	bDirty = false;

	if (bResized || !Texture)
	{
		// ponytail: a new texture per size change. Reuse a larger one if resizing every frame shows up in a profile.
		Texture = UTexture2D::CreateTransient(Size.X, Size.Y, PF_R8G8B8A8);
		Texture->SRGB = true;
		Texture->Filter = TF_Bilinear;
		Texture->AddressX = TA_Clamp;
		Texture->AddressY = TA_Clamp;
		Texture->LODGroup = TEXTUREGROUP_UI;
		Texture->NeverStream = true;

		FByteBulkData& Mip = Texture->GetPlatformData()->Mips[0].BulkData;
		FMemory::Memcpy(Mip.Lock(LOCK_READ_WRITE), Pixels.GetData(), Pixels.Num());
		Mip.Unlock();
		Texture->UpdateResource();
	}
	else
	{
		// The render thread reads the pixels later, so it gets its own copy, released once the upload is done.
		// ponytail: one copy per redrawn frame; the GPU renderer on the roadmap removes the upload altogether.
		struct FFrame
		{
			FUpdateTextureRegion2D Region;
			TArray<uint8> Pixels;
		};
		const TSharedRef<FFrame> Frame = MakeShared<FFrame>(FFrame{ FUpdateTextureRegion2D(0, 0, 0, 0, Size.X, Size.Y), Pixels });
		Texture->UpdateTextureRegions(0, 1, &Frame->Region, static_cast<uint32>(Size.X) * 4, 4, Frame->Pixels.GetData(),
			[Frame](uint8*, const FUpdateTextureRegion2D*) { Frame->Pixels.Empty(); });
	}
	return true;
}

TArray<FZuriProperty> UZuriInstance::GetProperties() const
{
	const TArray<FString> Names = Zuri::ReadNames(Handle.Get(), zuri_instance_property_count(Handle.Get()), zuri_instance_property_name);

	TArray<FZuriProperty> Properties;
	Properties.Reserve(Names.Num());
	for (int32 Index = 0; Index < Names.Num(); ++Index)
	{
		Properties.Add({ Names[Index], static_cast<EZuriPropertyType>(zuri_instance_property_type(Handle.Get(), static_cast<uint32>(Index))) });
	}
	return Properties;
}

bool UZuriInstance::WarnIfMissing(bool bFound, const TCHAR* Type, const FString& Name) const
{
	if (!bFound)
	{
		UE_LOG(LogZuri, Warning, TEXT("%s has no %s property called '%s'."), *GetNameSafe(File), Type, *Name);
	}
	return bFound;
}

bool UZuriInstance::SetNumber(const FString& Name, float Value)
{
	const FTCHARToUTF8 Utf8Name(*Name);
	return WarnIfMissing(zuri_instance_set_number(Handle.Get(), Utf8Name.Get(), Utf8Name.Length(), Value), TEXT("number"), Name);
}

float UZuriInstance::GetNumber(const FString& Name) const
{
	const FTCHARToUTF8 Utf8Name(*Name);
	float Value = 0.0f;
	zuri_instance_get_number(Handle.Get(), Utf8Name.Get(), Utf8Name.Length(), &Value);
	return Value;
}

bool UZuriInstance::SetBool(const FString& Name, bool bValue)
{
	// The runtime stores a bool as the number 0 or 1.
	return SetNumber(Name, bValue ? 1.0f : 0.0f);
}

bool UZuriInstance::GetBool(const FString& Name) const
{
	return GetNumber(Name) != 0.0f;
}

bool UZuriInstance::SetColor(const FString& Name, FLinearColor Color)
{
	return SetColor(Name, Color.ToFColorSRGB());
}

bool UZuriInstance::SetColor(const FString& Name, FColor Color)
{
	const FTCHARToUTF8 Utf8Name(*Name);
	return WarnIfMissing(zuri_instance_set_color(Handle.Get(), Utf8Name.Get(), Utf8Name.Length(), Color.ToPackedARGB()), TEXT("color"), Name);
}

FLinearColor UZuriInstance::GetColor(const FString& Name) const
{
	const FTCHARToUTF8 Utf8Name(*Name);
	uint32 Argb = 0;
	zuri_instance_get_color(Handle.Get(), Utf8Name.Get(), Utf8Name.Length(), &Argb);
	return FLinearColor::FromSRGBColor(FColor(Argb));
}

bool UZuriInstance::SetString(const FString& Name, const FString& Value)
{
	const FTCHARToUTF8 Utf8Name(*Name);
	const FTCHARToUTF8 Utf8Value(*Value);
	return WarnIfMissing(zuri_instance_set_string(Handle.Get(), Utf8Name.Get(), Utf8Name.Length(), Utf8Value.Get(), Utf8Value.Length()), TEXT("string"), Name);
}

FString UZuriInstance::GetString(const FString& Name) const
{
	const FTCHARToUTF8 Utf8Name(*Name);
	return Zuri::ReadString([this, &Utf8Name](char* Buffer, size_t Capacity)
	{
		return zuri_instance_get_string(Handle.Get(), Utf8Name.Get(), Utf8Name.Length(), Buffer, Capacity);
	});
}

bool UZuriInstance::FireTrigger(const FString& Name)
{
	const FTCHARToUTF8 Utf8Name(*Name);
	return WarnIfMissing(zuri_instance_fire(Handle.Get(), Utf8Name.Get(), Utf8Name.Length()), TEXT("trigger"), Name);
}

void UZuriInstance::Pointer(uint32 Kind, FVector2D Position)
{
	zuri_instance_pointer(Handle.Get(), Kind, static_cast<float>(Position.X), static_cast<float>(Position.Y));
}

void UZuriInstance::PointerMove(FVector2D Position)
{
	Pointer(ZURI_POINTER_MOVE, Position);
}

void UZuriInstance::PointerDown(FVector2D Position)
{
	Pointer(ZURI_POINTER_DOWN, Position);
}

void UZuriInstance::PointerUp(FVector2D Position)
{
	Pointer(ZURI_POINTER_UP, Position);
}

void UZuriInstance::PointerExit()
{
	Pointer(ZURI_POINTER_EXIT, FVector2D::ZeroVector);
}
