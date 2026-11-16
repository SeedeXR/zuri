#pragma once

#include "Components/Widget.h"
#include "CoreMinimal.h"
#include "UObject/ObjectPtr.h"
#include "ZuriInstance.h"

#include "ZuriWidget.generated.h"

class SZuriWidget;
class UZuriFile;

/**
 * Plays a Zuri artboard in UMG. It draws at the widget's size in pixels, so it stays sharp at any
 * DPI scale, redraws only when the picture changes, and feeds mouse and touch input, and with
 * Is Focusable the keyboard's, to the artboard's state machine. Set Visibility to Not Hit-Testable for decoration that should not take clicks.
 */
UCLASS(meta = (DisplayName = "Zuri"))
class ZURI_API UZuriWidget : public UWidget
{
	GENERATED_BODY()

public:
	/** The file to play. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Zuri")
	TObjectPtr<UZuriFile> File;

	/** Name of the artboard to show; empty for the file's first artboard. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Zuri")
	FString Artboard;

	/** Name of the state machine or animation to play; empty for the first state machine, or failing that the first animation. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Zuri")
	FString PlayName;

	/** How the artboard is placed when the widget has a different shape. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Zuri")
	EZuriFit Fit = EZuriFit::Contain;

	/** Where the artboard sits in the room the fit leaves over, or which part shows when it overflows. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Zuri")
	EZuriAlignment Alignment = EZuriAlignment::Center;

	/**
	 * Whether the widget takes the keyboard focus, on a click or by navigation. While it has the focus it gives
	 * the keys pressed to the artboard's state machine, and keeps only those the state machine reacts to.
	 */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Interaction")
	bool IsFocusable = false;

	/** Whether the sounds of events are left unplayed: see UZuriInstance::bMuted, which this starts each instance with. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Zuri")
	bool bMuted = false;

	/** Called for each event the artboard's state machine emits. */
	UPROPERTY(BlueprintAssignable, Category = "Zuri")
	FOnZuriEvent OnZuriEvent;

	/** Shows another file, artboard or state machine, starting a new instance. */
	UFUNCTION(BlueprintCallable, Category = "Zuri")
	void SetFile(UZuriFile* InFile, const FString& InArtboard = TEXT(""), const FString& InPlayName = TEXT(""));

	UFUNCTION(BlueprintCallable, Category = "Zuri")
	void SetFit(EZuriFit InFit);

	UFUNCTION(BlueprintCallable, Category = "Zuri")
	void SetAlignment(EZuriAlignment InAlignment);

	/**
	 * The playing instance, for its view model: Set Number, Set Color, Fire Trigger and the rest.
	 * It is created on first use; null if File is unset or invalid.
	 */
	UFUNCTION(BlueprintPure, Category = "Zuri")
	UZuriInstance* GetInstance();

	//~ Begin UWidget
	virtual void SynchronizeProperties() override;
	virtual void ReleaseSlateResources(bool bReleaseChildren) override;
#if WITH_EDITOR
	virtual const FText GetPaletteCategory() override;
	virtual void PostEditChangeProperty(FPropertyChangedEvent& PropertyChangedEvent) override;
#endif
	//~ End UWidget

protected:
	//~ Begin UWidget
	virtual TSharedRef<SWidget> RebuildWidget() override;
	//~ End UWidget

private:
	UFUNCTION()
	void HandleZuriEvent(FName EventName);

	/** Drops the instance; GetInstance makes the next one from the current properties. */
	void ResetInstance();

	UPROPERTY(Transient)
	TObjectPtr<UZuriInstance> Instance;

	TSharedPtr<SZuriWidget> MyWidget;
};
