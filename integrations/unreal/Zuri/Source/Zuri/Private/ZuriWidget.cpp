#include "ZuriWidget.h"

#include "SZuriWidget.h"
#include "ZuriFile.h"

#define LOCTEXT_NAMESPACE "Zuri"

UZuriInstance* UZuriWidget::GetInstance()
{
	if (!Instance && File)
	{
		Instance = UZuriInstance::Create(this, File, Artboard);
		if (Instance)
		{
			Instance->OnZuriEvent.AddDynamic(this, &UZuriWidget::HandleZuriEvent);
			Instance->SetFit(Fit);
			// An empty name plays the default, which a new instance is already doing.
			Instance->Play(PlayName);
		}
	}
	return Instance;
}

void UZuriWidget::ResetInstance()
{
	if (Instance)
	{
		Instance->OnZuriEvent.RemoveDynamic(this, &UZuriWidget::HandleZuriEvent);
		Instance = nullptr;
	}
}

void UZuriWidget::SetFile(UZuriFile* InFile, const FString& InArtboard, const FString& InPlayName)
{
	File = InFile;
	Artboard = InArtboard;
	PlayName = InPlayName;
	ResetInstance();
	if (MyWidget)
	{
		MyWidget->SetInstance(GetInstance());
	}
}

void UZuriWidget::SetFit(EZuriFit InFit)
{
	Fit = InFit;
	if (Instance)
	{
		Instance->SetFit(Fit);
	}
}

void UZuriWidget::HandleZuriEvent(FName EventName)
{
	OnZuriEvent.Broadcast(EventName);
}

TSharedRef<SWidget> UZuriWidget::RebuildWidget()
{
	MyWidget = SNew(SZuriWidget);
	return MyWidget.ToSharedRef();
}

void UZuriWidget::SynchronizeProperties()
{
	Super::SynchronizeProperties();
	SetFit(Fit);
	if (MyWidget)
	{
		MyWidget->SetInstance(GetInstance());
	}
}

void UZuriWidget::ReleaseSlateResources(bool bReleaseChildren)
{
	Super::ReleaseSlateResources(bReleaseChildren);
	MyWidget.Reset();
}

#if WITH_EDITOR
const FText UZuriWidget::GetPaletteCategory()
{
	return LOCTEXT("PaletteCategory", "Zuri");
}

void UZuriWidget::PostEditChangeProperty(FPropertyChangedEvent& PropertyChangedEvent)
{
	// Super synchronizes the properties, which starts an instance from the edited ones.
	ResetInstance();
	Super::PostEditChangeProperty(PropertyChangedEvent);
}
#endif

#undef LOCTEXT_NAMESPACE
