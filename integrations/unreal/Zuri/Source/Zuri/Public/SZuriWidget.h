#pragma once

#include "CoreMinimal.h"
#include "InputCoreTypes.h"
#include "Styling/SlateBrush.h"
#include "UObject/StrongObjectPtr.h"
#include "Widgets/DeclarativeSyntaxSupport.h"
#include "Widgets/SLeafWidget.h"
#include "ZuriInstance.h"

/**
 * Shows a Zuri instance. Each frame it advances the instance, redraws it at the widget's size in
 * pixels (local size x layout scale) when something changed, and it forwards the pointer to it.
 * A focusable widget also forwards the keys pressed while it has the keyboard focus, and keeps
 * only those the scene reacts to: the others still navigate the UI. A key that is held is sent
 * once, not for each repeat, and is let go when the widget loses the focus.
 */
class ZURI_API SZuriWidget : public SLeafWidget
{
public:
	SLATE_BEGIN_ARGS(SZuriWidget) : _IsFocusable(false) {}
		/** Whether the widget takes the keyboard focus, on a click or by navigation, and with it the keys. */
		SLATE_ARGUMENT(bool, IsFocusable)
	SLATE_END_ARGS()

	void Construct(const FArguments& InArgs);

	/** The instance to show; null shows nothing. */
	void SetInstance(UZuriInstance* InInstance);

	//~ Begin SWidget
	virtual void Tick(const FGeometry& AllottedGeometry, const double InCurrentTime, const float InDeltaTime) override;
	virtual int32 OnPaint(const FPaintArgs& Args, const FGeometry& AllottedGeometry, const FSlateRect& MyCullingRect, FSlateWindowElementList& OutDrawElements, int32 LayerId, const FWidgetStyle& InWidgetStyle, bool bParentEnabled) const override;
	virtual FVector2D ComputeDesiredSize(float LayoutScaleMultiplier) const override;
	virtual void OnMouseEnter(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent) override;
	virtual void OnMouseLeave(const FPointerEvent& MouseEvent) override;
	virtual FReply OnMouseMove(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent) override;
	virtual FReply OnMouseButtonDown(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent) override;
	virtual FReply OnMouseButtonUp(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent) override;
	virtual void OnMouseCaptureLost(const FCaptureLostEvent& CaptureLostEvent) override;
	virtual FReply OnMouseWheel(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent) override;
	virtual bool SupportsKeyboardFocus() const override;
	virtual FReply OnKeyDown(const FGeometry& MyGeometry, const FKeyEvent& InKeyEvent) override;
	virtual FReply OnKeyChar(const FGeometry& MyGeometry, const FCharacterEvent& InCharacterEvent) override;
	virtual FReply OnKeyUp(const FGeometry& MyGeometry, const FKeyEvent& InKeyEvent) override;
	virtual void OnFocusLost(const FFocusEvent& InFocusEvent) override;
	//~ End SWidget

private:
	/** A key that went down here and has not gone up. */
	struct FHeldKey
	{
		/** The name it was sent with, which its key-up is sent with too: a character key's is the character it typed. */
		FString Name;

		/** Whether the scene reacted to it going down; its repeats are then the scene's as well. */
		bool bHandled = false;
	};

	/** Sends a key going down to the instance and remembers it as held. */
	FReply PressKey(const FKey& Key, const FString& Name);

	/** Handled if Key is held and the scene reacted to it going down. */
	FReply RepeatKey(const FKey& Key) const;

	/** The pointer's position in pixels of the instance's render target. */
	FVector2D ToRenderTarget(const FGeometry& Geometry, const FPointerEvent& Event) const;

	/** The pointer whose press came through this widget and holds Slate's capture. */
	TOptional<int32> Pressed;

	/** Keeps the instance from being garbage collected while this widget shows it. */
	TStrongObjectPtr<UZuriInstance> Instance;

	/** Draws the instance's texture. */
	FSlateBrush Brush;

	bool bIsFocusable = false;

	/** The key that last went down, if it is one that types a character: OnKeyChar, which follows, says which. */
	FKey PendingKey;

	TMap<FKey, FHeldKey> HeldKeys;
};
