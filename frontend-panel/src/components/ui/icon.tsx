import type { ComponentProps, ComponentType } from "react";
import { FaDocker } from "react-icons/fa6";
import {
	PiArrowClockwise,
	PiArrowCounterClockwise,
	PiArrowDown,
	PiArrowLeft,
	PiArrowRight,
	PiArrowSquareOut,
	PiArrowsClockwise,
	PiArrowsInCardinal,
	PiArrowsOutCardinal,
	PiArrowUp,
	PiArrowUUpLeft,
	PiBracketsCurly,
	PiCaretDown,
	PiCaretLeft,
	PiCaretRight,
	PiCaretUp,
	PiCheck,
	PiClipboardText,
	PiClockCounterClockwise,
	PiCloud,
	PiCopy,
	PiCpu,
	PiDotsThree,
	PiDownloadSimple,
	PiEnvelopeSimple,
	PiEye,
	PiEyeSlash,
	PiFile,
	PiFileAudio,
	PiFileCode,
	PiFileImage,
	PiFilePlus,
	PiFileText,
	PiFileVideo,
	PiFileZip,
	PiFloppyDisk,
	PiFolder,
	PiFolderOpen,
	PiFolderPlus,
	PiGauge,
	PiGear,
	PiGlobe,
	PiGridFour,
	PiHardDrive,
	PiHouse,
	PiInfo,
	PiKey,
	PiLink,
	PiLinkSimple,
	PiList,
	PiListBullets,
	PiListChecks,
	PiLock,
	PiLockOpen,
	PiMagnifyingGlass,
	PiMagnifyingGlassMinus,
	PiMagnifyingGlassPlus,
	PiMinus,
	PiMonitor,
	PiMoon,
	PiMusicNotes,
	PiPause,
	PiPencilSimple,
	PiPlay,
	PiPlus,
	PiPower,
	PiPresentation,
	PiQueue,
	PiRepeat,
	PiRepeatOnce,
	PiScroll,
	PiShield,
	PiShuffle,
	PiSignIn,
	PiSignOut,
	PiSkipBack,
	PiSkipForward,
	PiSortAscending,
	PiSortDescending,
	PiSpeakerHigh,
	PiSpeakerSlash,
	PiSpinner,
	PiSun,
	PiTable,
	PiTrash,
	PiUploadSimple,
	PiUser,
	PiVinylRecord,
	PiWarning,
	PiWarningCircle,
	PiWifiHigh,
	PiWifiX,
	PiWrench,
	PiX,
} from "react-icons/pi";

export type IconName =
	| "ArrowCounterClockwise"
	| "ArrowClockwise"
	| "ArrowDown"
	| "ArrowLeft"
	| "ArrowRight"
	| "ArrowSquareOut"
	| "ArrowUp"
	| "ArrowsInCardinal"
	| "ArrowsClockwise"
	| "ArrowsOutCardinal"
	| "BracketsCurly"
	| "CaretDown"
	| "CaretLeft"
	| "CaretRight"
	| "CaretUp"
	| "Check"
	| "CircleAlert"
	| "ClipboardText"
	| "Clock"
	| "Cloud"
	| "Copy"
	| "Cpu"
	| "Docker"
	| "DotsThree"
	| "Download"
	| "EnvelopeSimple"
	| "Eye"
	| "EyeSlash"
	| "File"
	| "FileAudio"
	| "FileCode"
	| "FileImage"
	| "FilePlus"
	| "FileText"
	| "FileVideo"
	| "FileZip"
	| "FloppyDisk"
	| "Folder"
	| "FolderOpen"
	| "FolderPlus"
	| "Gauge"
	| "Gear"
	| "Globe"
	| "Grid"
	| "HardDrive"
	| "House"
	| "Info"
	| "Key"
	| "Link"
	| "LinkSimple"
	| "List"
	| "ListBullets"
	| "ListChecks"
	| "Lock"
	| "LockOpen"
	| "MagnifyingGlass"
	| "MagnifyingGlassMinus"
	| "MagnifyingGlassPlus"
	| "Monitor"
	| "Moon"
	| "Minus"
	| "Pause"
	| "MusicNotes"
	| "PencilSimple"
	| "Play"
	| "Plus"
	| "Power"
	| "Presentation"
	| "Queue"
	| "Repeat"
	| "RepeatOnce"
	| "Scroll"
	| "Shield"
	| "SignIn"
	| "SignOut"
	| "Shuffle"
	| "SkipBack"
	| "SkipForward"
	| "SortAscending"
	| "SortDescending"
	| "SpeakerHigh"
	| "SpeakerSlash"
	| "Spinner"
	| "Sun"
	| "Table"
	| "Trash"
	| "Undo"
	| "Upload"
	| "User"
	| "VinylRecord"
	| "Warning"
	| "WifiHigh"
	| "WifiX"
	| "Wrench"
	| "X";

const iconMap: Record<IconName, ComponentType<{ className?: string }>> = {
	ArrowCounterClockwise: PiArrowCounterClockwise,
	ArrowClockwise: PiArrowClockwise,
	ArrowDown: PiArrowDown,
	ArrowLeft: PiArrowLeft,
	ArrowRight: PiArrowRight,
	ArrowSquareOut: PiArrowSquareOut,
	ArrowUp: PiArrowUp,
	ArrowsInCardinal: PiArrowsInCardinal,
	ArrowsClockwise: PiArrowsClockwise,
	ArrowsOutCardinal: PiArrowsOutCardinal,
	BracketsCurly: PiBracketsCurly,
	CaretDown: PiCaretDown,
	CaretLeft: PiCaretLeft,
	CaretRight: PiCaretRight,
	CaretUp: PiCaretUp,
	Check: PiCheck,
	CircleAlert: PiWarningCircle,
	ClipboardText: PiClipboardText,
	Clock: PiClockCounterClockwise,
	Cloud: PiCloud,
	Copy: PiCopy,
	Cpu: PiCpu,
	Docker: FaDocker,
	DotsThree: PiDotsThree,
	Download: PiDownloadSimple,
	EnvelopeSimple: PiEnvelopeSimple,
	Eye: PiEye,
	EyeSlash: PiEyeSlash,
	File: PiFile,
	FileAudio: PiFileAudio,
	FileCode: PiFileCode,
	FileImage: PiFileImage,
	FilePlus: PiFilePlus,
	FileText: PiFileText,
	FileVideo: PiFileVideo,
	FileZip: PiFileZip,
	FloppyDisk: PiFloppyDisk,
	Folder: PiFolder,
	FolderOpen: PiFolderOpen,
	FolderPlus: PiFolderPlus,
	Gauge: PiGauge,
	Gear: PiGear,
	Globe: PiGlobe,
	Grid: PiGridFour,
	HardDrive: PiHardDrive,
	House: PiHouse,
	Info: PiInfo,
	Key: PiKey,
	Link: PiLink,
	LinkSimple: PiLinkSimple,
	List: PiList,
	ListBullets: PiListBullets,
	ListChecks: PiListChecks,
	Lock: PiLock,
	LockOpen: PiLockOpen,
	MagnifyingGlass: PiMagnifyingGlass,
	MagnifyingGlassMinus: PiMagnifyingGlassMinus,
	MagnifyingGlassPlus: PiMagnifyingGlassPlus,
	Monitor: PiMonitor,
	Moon: PiMoon,
	Minus: PiMinus,
	MusicNotes: PiMusicNotes,
	Pause: PiPause,
	PencilSimple: PiPencilSimple,
	Play: PiPlay,
	Plus: PiPlus,
	Power: PiPower,
	Presentation: PiPresentation,
	Queue: PiQueue,
	Repeat: PiRepeat,
	RepeatOnce: PiRepeatOnce,
	Scroll: PiScroll,
	Shield: PiShield,
	SignIn: PiSignIn,
	SignOut: PiSignOut,
	Shuffle: PiShuffle,
	SkipBack: PiSkipBack,
	SkipForward: PiSkipForward,
	SortAscending: PiSortAscending,
	SortDescending: PiSortDescending,
	SpeakerHigh: PiSpeakerHigh,
	SpeakerSlash: PiSpeakerSlash,
	Spinner: PiSpinner,
	Sun: PiSun,
	Table: PiTable,
	Trash: PiTrash,
	Undo: PiArrowUUpLeft,
	Upload: PiUploadSimple,
	User: PiUser,
	VinylRecord: PiVinylRecord,
	Warning: PiWarning,
	WifiHigh: PiWifiHigh,
	WifiX: PiWifiX,
	Wrench: PiWrench,
	X: PiX,
};

export interface IconProps {
	name: IconName;
	className?: string;
}

export function isIconName(value: string): value is IconName {
	return Object.hasOwn(iconMap, value);
}

export function Icon({
	name,
	className,
	...props
}: IconProps & ComponentProps<"svg">) {
	const IconComponent = iconMap[name];
	if (!IconComponent) return null;
	return <IconComponent className={className} {...props} />;
}
