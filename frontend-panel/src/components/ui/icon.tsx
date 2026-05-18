import type { ComponentProps, ComponentType } from "react";
import { FaDocker } from "react-icons/fa6";
import {
	PiArrowClockwise,
	PiArrowCounterClockwise,
	PiArrowDown,
	PiArrowLeft,
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
	PiGear,
	PiGlobe,
	PiGridFour,
	PiHardDrive,
	PiHouse,
	PiInfo,
	PiLink,
	PiLinkSimple,
	PiList,
	PiListBullets,
	PiLock,
	PiLockOpen,
	PiMagnifyingGlass,
	PiMinus,
	PiMonitor,
	PiMoon,
	PiPause,
	PiPencilSimple,
	PiPlay,
	PiPlus,
	PiPower,
	PiPresentation,
	PiScroll,
	PiShield,
	PiSignIn,
	PiSignOut,
	PiSortAscending,
	PiSortDescending,
	PiSpinner,
	PiSun,
	PiTable,
	PiTrash,
	PiUploadSimple,
	PiUser,
	PiWarning,
	PiWarningCircle,
	PiWifiHigh,
	PiWifiX,
	PiX,
} from "react-icons/pi";

export type IconName =
	| "ArrowCounterClockwise"
	| "ArrowClockwise"
	| "ArrowDown"
	| "ArrowLeft"
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
	| "Gear"
	| "Globe"
	| "Grid"
	| "HardDrive"
	| "House"
	| "Info"
	| "Link"
	| "LinkSimple"
	| "List"
	| "ListBullets"
	| "Lock"
	| "LockOpen"
	| "MagnifyingGlass"
	| "Monitor"
	| "Moon"
	| "Minus"
	| "Pause"
	| "PencilSimple"
	| "Play"
	| "Plus"
	| "Power"
	| "Presentation"
	| "Scroll"
	| "Shield"
	| "SignIn"
	| "SignOut"
	| "SortAscending"
	| "SortDescending"
	| "Spinner"
	| "Sun"
	| "Table"
	| "Trash"
	| "Undo"
	| "Upload"
	| "User"
	| "Warning"
	| "WifiHigh"
	| "WifiX"
	| "X";

const iconMap: Record<IconName, ComponentType<{ className?: string }>> = {
	ArrowCounterClockwise: PiArrowCounterClockwise,
	ArrowClockwise: PiArrowClockwise,
	ArrowDown: PiArrowDown,
	ArrowLeft: PiArrowLeft,
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
	Gear: PiGear,
	Globe: PiGlobe,
	Grid: PiGridFour,
	HardDrive: PiHardDrive,
	House: PiHouse,
	Info: PiInfo,
	Link: PiLink,
	LinkSimple: PiLinkSimple,
	List: PiList,
	ListBullets: PiListBullets,
	Lock: PiLock,
	LockOpen: PiLockOpen,
	MagnifyingGlass: PiMagnifyingGlass,
	Monitor: PiMonitor,
	Moon: PiMoon,
	Minus: PiMinus,
	Pause: PiPause,
	PencilSimple: PiPencilSimple,
	Play: PiPlay,
	Plus: PiPlus,
	Power: PiPower,
	Presentation: PiPresentation,
	Scroll: PiScroll,
	Shield: PiShield,
	SignIn: PiSignIn,
	SignOut: PiSignOut,
	SortAscending: PiSortAscending,
	SortDescending: PiSortDescending,
	Spinner: PiSpinner,
	Sun: PiSun,
	Table: PiTable,
	Trash: PiTrash,
	Undo: PiArrowUUpLeft,
	Upload: PiUploadSimple,
	User: PiUser,
	Warning: PiWarning,
	WifiHigh: PiWifiHigh,
	WifiX: PiWifiX,
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
