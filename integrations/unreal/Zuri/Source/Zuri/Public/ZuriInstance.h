#pragma once

#include "CoreMinimal.h"
#include "Templates/UniquePtr.h"
#include "UObject/Object.h"
#include "UObject/ObjectPtr.h"

#include "ZuriInstance.generated.h"

class USoundWave;
class UTexture2D;
class UZuriFile;
struct ZuriInstance;

/** How an artboard is placed in a render target of a different shape. */
UENUM(BlueprintType)
enum class EZuriFit : uint8
{
	/** Scale uniformly so the whole artboard is visible. */
	Contain,
	/** Scale uniformly so the artboard covers the target, cropping the overflow. */
	Cover,
	/** Stretch to the target, ignoring the aspect ratio. */
	Fill,
	/** No scaling: the artboard keeps its own size. */
	None,
	/** Scale uniformly so the artboard is as wide as the target. */
	FitWidth,
	/** Scale uniformly so the artboard is as tall as the target. */
	FitHeight,
	/** Contain when the artboard is larger than the target, its own size otherwise. */
	ScaleDown,
};

/** Where the artboard sits in the room a fit leaves over, or which part of it shows when it overflows the target. */
UENUM(BlueprintType)
enum class EZuriAlignment : uint8
{
	Center,
	TopLeft,
	TopCenter,
	TopRight,
	CenterLeft,
	CenterRight,
	BottomLeft,
	BottomCenter,
	BottomRight,
};

/** The type of a view-model property. */
UENUM(BlueprintType)
enum class EZuriPropertyType : uint8
{
	Number,
	Bool,
	Trigger,
	Color,
	String,
	/** One of the property's Options: a number by its position (from 0), a string by its name. */
	Enum,
};

/** One view-model property of an artboard. */
USTRUCT(BlueprintType)
struct FZuriProperty
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly, Category = "Zuri")
	FString Name;

	UPROPERTY(BlueprintReadOnly, Category = "Zuri")
	EZuriPropertyType Type = EZuriPropertyType::Number;

	/** The names of an enum property's Options, in order; empty for the other types. */
	UPROPERTY(BlueprintReadOnly, Category = "Zuri")
	TArray<FString> Options;
};

DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnZuriEvent, FName, EventName);
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOnZuriSound, USoundWave*, Sound, float, Volume);

/** Releases a runtime instance handle. */
struct FZuriInstanceDeleter
{
	ZURI_API void operator()(ZuriInstance* Handle) const;
};

/**
 * One playing copy of an artboard, drawn into a texture.
 *
 * Each frame call Advance and then Render; Render does nothing when the picture has not changed.
 * UZuriWidget and UZuriComponent do this for you. Property and event names are case sensitive.
 * Use an instance on the game thread only.
 */
UCLASS(BlueprintType)
class ZURI_API UZuriInstance : public UObject
{
	GENERATED_BODY()

public:
	/**
	 * Creates an instance of a file's artboard and starts its first state machine, or failing that its first animation.
	 * @param Artboard Name of the artboard; empty for the first one.
	 * @return null, with the reason in the log, if the file is invalid or has no such artboard.
	 */
	UFUNCTION(BlueprintCallable, Category = "Zuri", meta = (DisplayName = "Create Zuri Instance", DefaultToSelf = "Outer"))
	static UZuriInstance* Create(UObject* Outer, UZuriFile* File, const FString& Artboard = TEXT(""));

	/** Like Create, choosing the artboard by its index in the file. */
	UFUNCTION(BlueprintCallable, Category = "Zuri", meta = (DisplayName = "Create Zuri Instance By Index", DefaultToSelf = "Outer"))
	static UZuriInstance* CreateByIndex(UObject* Outer, UZuriFile* File, int32 ArtboardIndex);

	/** The file this instance was created from. */
	UFUNCTION(BlueprintPure, Category = "Zuri")
	UZuriFile* GetFile() const { return File; }

	/** Size of the artboard in its own units. */
	UFUNCTION(BlueprintPure, Category = "Zuri")
	FVector2D GetArtboardSize() const;

	UFUNCTION(BlueprintPure, Category = "Zuri|Playback")
	TArray<FString> GetStateMachineNames() const;

	UFUNCTION(BlueprintPure, Category = "Zuri|Playback")
	TArray<FString> GetAnimationNames() const;

	/**
	 * Plays the state machine called Name, or failing that the animation called Name.
	 * An empty name plays the first state machine, or failing that the first animation.
	 * @return false if nothing has that name.
	 */
	UFUNCTION(BlueprintCallable, Category = "Zuri|Playback")
	bool Play(const FString& Name = TEXT(""));

	/**
	 * Jumps the animation that is playing to Seconds on its timeline; the next Advance (0 is fine) shows it.
	 * Nested artboards restart and run to the same time.
	 * @return false if no animation is playing directly: a state machine has no single timeline.
	 */
	UFUNCTION(BlueprintCallable, Category = "Zuri|Playback")
	bool Seek(float Seconds);

	/** The object id of the state each layer of the playing state machine is in, -1 for none. Empty unless a state machine is playing. */
	UFUNCTION(BlueprintPure, Category = "Zuri|Playback")
	TArray<int32> GetLayerStates() const;

	/**
	 * Advances time and broadcasts the events emitted since the last call.
	 * @return true if the picture may have changed.
	 */
	UFUNCTION(BlueprintCallable, Category = "Zuri|Playback")
	bool Advance(float DeltaSeconds);

	/**
	 * Draws into the texture at Width x Height pixels. Skipped when nothing has changed since the last draw at that size.
	 * @return true if the texture was redrawn. A different size replaces the texture, so read GetTexture again.
	 */
	UFUNCTION(BlueprintCallable, Category = "Zuri|Rendering")
	bool Render(int32 Width, int32 Height);

	/** The texture Render draws into: sRGB, straight alpha. Null until the first Render. */
	UFUNCTION(BlueprintPure, Category = "Zuri|Rendering")
	UTexture2D* GetTexture() const { return Texture; }

	UFUNCTION(BlueprintPure, Category = "Zuri|Rendering")
	EZuriFit GetFit() const { return Fit; }

	UFUNCTION(BlueprintCallable, Category = "Zuri|Rendering")
	void SetFit(EZuriFit InFit);

	UFUNCTION(BlueprintPure, Category = "Zuri|Rendering")
	EZuriAlignment GetAlignment() const { return Alignment; }

	UFUNCTION(BlueprintCallable, Category = "Zuri|Rendering")
	void SetAlignment(EZuriAlignment InAlignment);

	/** Size in pixels of the last Render. */
	FIntPoint GetRenderSize() const { return RenderSize; }

	/** Pixels of the last Render: RGBA8, straight alpha, rows top to bottom. */
	TConstArrayView<uint8> GetPixels() const { return Pixels; }

	/** The artboard's view-model properties. */
	UFUNCTION(BlueprintPure, Category = "Zuri|View Model")
	TArray<FZuriProperty> GetProperties() const;

	/**
	 * The view-model properties of a nested artboard, by its path: "card" is the artboard nested under that
	 * name, "rows/2" the third copy of a list ("rows" is its first). Empty where nothing is nested by that
	 * path, which a list without copies is too. Each is set as the path, "/" and its name.
	 */
	UFUNCTION(BlueprintPure, Category = "Zuri|View Model")
	TArray<FZuriProperty> GetPropertiesIn(const FString& Artboard) const;

	/**
	 * Whether there is a view-model property called Name. Name can be a path into a nested artboard:
	 * "card/title" is the property "title" of the artboard nested under the name "card". Every getter and
	 * setter here takes such a path; Get Properties lists the outer artboard's, Get Properties In those
	 * of one nested in it.
	 */
	UFUNCTION(BlueprintPure, Category = "Zuri|View Model")
	bool HasProperty(const FString& Name) const;

	/**
	 * Setters return false, and warn in the log, if the artboard has no property called Name.
	 * For an enum property the number is the position of an Option, from 0.
	 */
	UFUNCTION(BlueprintCallable, Category = "Zuri|View Model")
	bool SetNumber(const FString& Name, float Value);

	/** Getters return a zero value if the artboard has no property called Name. */
	UFUNCTION(BlueprintPure, Category = "Zuri|View Model")
	float GetNumber(const FString& Name) const;

	UFUNCTION(BlueprintCallable, Category = "Zuri|View Model")
	bool SetBool(const FString& Name, bool bValue);

	UFUNCTION(BlueprintPure, Category = "Zuri|View Model")
	bool GetBool(const FString& Name) const;

	UFUNCTION(BlueprintCallable, Category = "Zuri|View Model")
	bool SetColor(const FString& Name, FLinearColor Color);

	/** Sets a color property from an sRGB color, exactly as authored. */
	bool SetColor(const FString& Name, FColor Color);

	UFUNCTION(BlueprintPure, Category = "Zuri|View Model")
	FLinearColor GetColor(const FString& Name) const;

	/** For an enum property the string is the name of an Option; any other name is refused. */
	UFUNCTION(BlueprintCallable, Category = "Zuri|View Model")
	bool SetString(const FString& Name, const FString& Value);

	/** A string property's text, or the name of the Option an enum property is on. */
	UFUNCTION(BlueprintPure, Category = "Zuri|View Model")
	FString GetString(const FString& Name) const;

	UFUNCTION(BlueprintCallable, Category = "Zuri|View Model")
	bool FireTrigger(const FString& Name);

	/**
	 * Pointer input, in pixels of the last Render with the origin at the top left. A press that
	 * moves a Layout that scrolls is a scroll, and until it is let go the scene's Listeners hear
	 * only that the pointer has left. Call PointerUp or PointerExit for every PointerDown: until
	 * then the press is open and the Layouts it holds follow every PointerMove.
	 *
	 * The scene has one pointer and one button, and the runtime keeps to them: with several
	 * (fingers, a mouse and its buttons), pass each one's events with its Pointer, your number for
	 * it, and the Button that went down or up (0 is the primary one). A press begins when a primary
	 * button goes down and ends when it goes up or its pointer leaves; meanwhile no other pointer
	 * is the scene's. Each call returns whether the event was the scene's.
	 */
	UFUNCTION(BlueprintCallable, Category = "Zuri|Input")
	bool PointerMove(FVector2D Position, int32 Pointer = 0);

	UFUNCTION(BlueprintCallable, Category = "Zuri|Input")
	bool PointerDown(FVector2D Position, int32 Pointer = 0, int32 Button = 0);

	UFUNCTION(BlueprintCallable, Category = "Zuri|Input")
	bool PointerUp(FVector2D Position, int32 Pointer = 0, int32 Button = 0);

	/** The pointer left the surface. */
	UFUNCTION(BlueprintCallable, Category = "Zuri|Input")
	bool PointerExit(int32 Pointer = 0);

	/**
	 * How far, in pixels of the last Render, a press may go along a Layout that scrolls before the
	 * Layout goes with it and the press is a scroll, no longer a tap. 8 until set. Give a finger
	 * more than a mouse, and a dense screen more pixels; a negative number is not taken. It is
	 * this instance's: set it again on a new one.
	 */
	UFUNCTION(BlueprintCallable, Category = "Zuri|Input")
	void SetDragThreshold(float Distance);

	/**
	 * A key went down or up, for the scene's keyDown and keyUp listeners.
	 * @param KeyName The key as the web's KeyboardEvent.key names it, case sensitive: "Enter", "Escape", "Tab",
	 * "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Backspace", "Delete", "Home", "End", "PageUp", "PageDown",
	 * " " for space, or the character typed, such as "a", "A" or "1".
	 * @return true if a listener reacted; the next Advance and Render show it.
	 */
	UFUNCTION(BlueprintCallable, Category = "Zuri|Input")
	bool Key(const FString& KeyName, bool bDown);

	/** Broadcast from Advance for each event the state machine emitted. */
	UPROPERTY(BlueprintAssignable, Category = "Zuri")
	FOnZuriEvent OnZuriEvent;

	/**
	 * Whether Advance leaves the sounds of events unplayed. They are played as 2D sounds, in a game world
	 * only (not in the UMG designer), so mute the instance to play them your own way from OnZuriSound.
	 */
	UPROPERTY(BlueprintReadWrite, Category = "Zuri")
	bool bMuted = false;

	/**
	 * Broadcast from Advance, before OnZuriEvent, for each event that has a sound, muted or not:
	 * the sound and its volume, 0 to 1. To play it at a place in the world, say, mute the instance and play it from here.
	 */
	UPROPERTY(BlueprintAssignable, Category = "Zuri")
	FOnZuriSound OnZuriSound;

	/** Names of the events OnZuriEvent can report: the artboard's own and those of artboards nested in it. */
	UFUNCTION(BlueprintPure, Category = "Zuri")
	TArray<FString> GetDeclaredEvents() const;

	/**
	 * The value an object draws with, after animation and binds, for a schema property key: numbers, bools and enums.
	 * Object ids are file indices. @return false if the object has no such property.
	 */
	bool GetObjectProperty(uint32 Object, uint32 Key, float& OutValue) const;

	/** Like GetObjectProperty, for color properties. */
	bool GetObjectPropertyColor(uint32 Object, uint32 Key, FColor& OutColor) const;

	//~ Begin UObject
	virtual void BeginDestroy() override;
	//~ End UObject

private:
	bool PointerEvent(uint32 Kind, FVector2D Position, int32 Pointer, int32 Button = 0);

	/** Warns that the artboard has no Type property called Name; setters report failure through it. */
	bool WarnIfMissing(bool bFound, const TCHAR* Type, const FString& Name) const;

	TUniquePtr<ZuriInstance, FZuriInstanceDeleter> Handle;

	UPROPERTY()
	TObjectPtr<UZuriFile> File;

	/** The file's sounds as they were when this instance was made: it goes on playing that version after a reimport, whose sounds have other ids. */
	UPROPERTY()
	TMap<uint32, TObjectPtr<USoundWave>> Sounds;

	UPROPERTY(Transient)
	TObjectPtr<UTexture2D> Texture;

	TArray<uint8> Pixels;
	FIntPoint RenderSize = FIntPoint::ZeroValue;
	EZuriFit Fit = EZuriFit::Contain;
	EZuriAlignment Alignment = EZuriAlignment::Center;

	/** Something changed since the last successful Render. */
	bool bDirty = true;
};
