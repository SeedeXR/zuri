#pragma once

#include "Containers/Array.h"
#include "Containers/ContainerAllocationPolicies.h"
#include "Containers/StringConv.h"
#include "Containers/UnrealString.h"
#include "CoreTypes.h"
#include "Logging/LogMacros.h"

#include "zuri.h"

DECLARE_LOG_CATEGORY_EXTERN(LogZuri, Log, All);

/** Helpers for the C API's conventions. Every call into the runtime is made on the game thread. */
namespace Zuri
{
	/** Reads a "string out" value: Read(Buffer, Capacity) writes UTF-8 and returns its full length. */
	template <typename ReadType>
	FString ReadString(ReadType&& Read)
	{
		TArray<UTF8CHAR, TInlineAllocator<128>> Buffer;
		Buffer.SetNumUninitialized(static_cast<int32>(Read(nullptr, 0)));
		Read(reinterpret_cast<char*>(Buffer.GetData()), static_cast<size_t>(Buffer.Num()));
		return FString::ConstructFromPtrSize(Buffer.GetData(), Buffer.Num());
	}

	/** Reads an indexed list of names, such as a file's artboards, through one of the C API's zuri_*_name functions. */
	template <typename HandleType>
	TArray<FString> ReadNames(const HandleType* Handle, uint32 Count, size_t (*ReadName)(const HandleType*, uint32_t, char*, size_t))
	{
		TArray<FString> Names;
		Names.Reserve(static_cast<int32>(Count));
		for (uint32 Index = 0; Index < Count; ++Index)
		{
			Names.Add(ReadString([&](char* Buffer, size_t Capacity) { return ReadName(Handle, Index, Buffer, Capacity); }));
		}
		return Names;
	}

	/** Why the last failed runtime call on this thread failed. */
	inline FString LastError()
	{
		return ReadString([](char* Buffer, size_t Capacity) { return zuri_last_error(Buffer, Capacity); });
	}

	/** Index of Name in Names, or INDEX_NONE. Zuri names are case sensitive, unlike FString's operator==. */
	inline int32 FindName(const TArray<FString>& Names, const FString& Name)
	{
		return Names.IndexOfByPredicate([&Name](const FString& Candidate) { return Candidate.Equals(Name, ESearchCase::CaseSensitive); });
	}
}
