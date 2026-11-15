#include "ZuriFileFactory.h"

#include "EditorFramework/AssetImportData.h"
#include "Misc/FeedbackContext.h"
#include "Misc/FileHelper.h"
#include "ZuriFile.h"

UZuriFileFactory::UZuriFileFactory()
{
	SupportedClass = UZuriFile::StaticClass();
	Formats.Add(TEXT("zur;Zuri file"));
	bEditorImport = true;
}

UObject* UZuriFileFactory::FactoryCreateBinary(UClass* InClass, UObject* InParent, FName InName, EObjectFlags Flags, UObject* Context, const TCHAR* Type, const uint8*& Buffer, const uint8* BufferEnd, FFeedbackContext* Warn)
{
	UZuriFile* File = NewObject<UZuriFile>(InParent, InClass, InName, Flags);
	if (!File->SetData(TArray<uint8>(Buffer, static_cast<int32>(BufferEnd - Buffer))))
	{
		Warn->Logf(ELogVerbosity::Error, TEXT("%s is not a valid Zuri file; LogZuri has the reason."), *GetCurrentFilename());
		File->MarkAsGarbage();
		return nullptr;
	}
	File->AssetImportData->Update(GetCurrentFilename());
	return File;
}

bool UZuriFileFactory::CanReimport(UObject* Obj, TArray<FString>& OutFilenames)
{
	const UZuriFile* File = Cast<UZuriFile>(Obj);
	if (!File || !File->AssetImportData)
	{
		return false;
	}
	File->AssetImportData->ExtractFilenames(OutFilenames);
	return true;
}

void UZuriFileFactory::SetReimportPaths(UObject* Obj, const TArray<FString>& NewReimportPaths)
{
	UZuriFile* File = Cast<UZuriFile>(Obj);
	if (File && File->AssetImportData && NewReimportPaths.Num() == 1)
	{
		File->AssetImportData->UpdateFilenameOnly(NewReimportPaths[0]);
	}
}

EReimportResult::Type UZuriFileFactory::Reimport(UObject* Obj)
{
	UZuriFile* File = Cast<UZuriFile>(Obj);
	if (!File || !File->AssetImportData)
	{
		return EReimportResult::Failed;
	}

	const FString Filename = File->AssetImportData->GetFirstFilename();
	TArray<uint8> Bytes;
	if (!FFileHelper::LoadFileToArray(Bytes, *Filename) || !File->SetData(MoveTemp(Bytes)))
	{
		return EReimportResult::Failed;
	}
	File->AssetImportData->Update(Filename);
	File->MarkPackageDirty();
	return EReimportResult::Succeeded;
}
