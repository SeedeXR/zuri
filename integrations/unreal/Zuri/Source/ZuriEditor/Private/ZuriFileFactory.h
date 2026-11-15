#pragma once

#include "CoreMinimal.h"
#include "EditorReimportHandler.h"
#include "Factories/Factory.h"

#include "ZuriFileFactory.generated.h"

/** Imports and reimports .zur files as Zuri File assets. */
UCLASS(hidecategories = Object)
class UZuriFileFactory : public UFactory, public FReimportHandler
{
	GENERATED_BODY()

public:
	UZuriFileFactory();

	//~ Begin UFactory
	virtual UObject* FactoryCreateBinary(UClass* InClass, UObject* InParent, FName InName, EObjectFlags Flags, UObject* Context, const TCHAR* Type, const uint8*& Buffer, const uint8* BufferEnd, FFeedbackContext* Warn) override;
	//~ End UFactory

	//~ Begin FReimportHandler
	virtual bool CanReimport(UObject* Obj, TArray<FString>& OutFilenames) override;
	virtual void SetReimportPaths(UObject* Obj, const TArray<FString>& NewReimportPaths) override;
	virtual EReimportResult::Type Reimport(UObject* Obj) override;
	//~ End FReimportHandler
};
