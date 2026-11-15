#pragma once

#include "CoreMinimal.h"
#include "Styling/SlateBrush.h"
#include "UObject/StrongObjectPtr.h"
#include "Widgets/DeclarativeSyntaxSupport.h"
#include "Widgets/SLeafWidget.h"
#include "ZuriInstance.h"

/**
 * Shows a Zuri instance. Each frame it advances the instance, redraws it at the widget's size in
 * pixels (local size x layout scale) when something changed, and it forwards the pointer to it.
 */
class ZURI_API SZuriWidget : public SLeafWidget
{
public:
	SLATE_BEGIN_ARGS(SZuriWidget) {}
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
	//~ End SWidget

private:
	/** The pointer's position in pixels of the instance's render target. */
	FVector2D ToRenderTarget(const FGeometry& Geometry, const FPointerEvent& Event) const;

	/** Keeps the instance from being garbage collected while this widget shows it. */
	TStrongObjectPtr<UZuriInstance> Instance;

	/** Draws the instance's texture. */
	FSlateBrush Brush;
};
