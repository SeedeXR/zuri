#pragma once

#include "AssetDefinitionDefault.h"
#include "CoreMinimal.h"
#include "ZuriFile.h"

#include "AssetDefinition_ZuriFile.generated.h"

/** How a Zuri file appears in the Content Browser. */
UCLASS()
class UAssetDefinition_ZuriFile : public UAssetDefinitionDefault
{
	GENERATED_BODY()

public:
	//~ Begin UAssetDefinition
	virtual FText GetAssetDisplayName() const override { return NSLOCTEXT("Zuri", "ZuriFileName", "Zuri File"); }
	virtual FLinearColor GetAssetColor() const override { return FColor(79, 124, 255); }
	virtual TSoftClassPtr<UObject> GetAssetClass() const override { return UZuriFile::StaticClass(); }
	virtual TConstArrayView<FAssetCategoryPath> GetAssetCategories() const override
	{
		static const FAssetCategoryPath Categories[] = { EAssetCategoryPaths::UI };
		return Categories;
	}
	virtual bool CanImport() const override { return true; }
	//~ End UAssetDefinition
};
