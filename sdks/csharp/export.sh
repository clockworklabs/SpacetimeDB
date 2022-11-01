#!/bin/bash

if [ -z "$UNITY_PATH" ]; then
	export UNITY_PATH="~/Unity/Hub/Editor/2020.3.25f1/Editor/Unity"
fi
	
if [ ! -f $UNITY_PATH ]; then
	echo "Unable to find a version of Unity. Please specify it using UNITY_PATH :)"	
	exit 1
fi

$UNITY_PATH -projectPath . -quit -batchmode -nographics -ignorecompilererrors -executeMethod "ExportPackage.Export"
